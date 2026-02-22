use crate::imports::*;
use crate::runtime::services::LogStores;
use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    routing::get,
};
use serde::Serialize;
use std::convert::Infallible;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_postgres::NoTls;
use tokio_stream::wrappers::IntervalStream;

const TABLE_STATS_SQL: &str = r#"
SELECT
    c.relname AS table_name,
    GREATEST(
        COALESCE(s.n_live_tup, 0)::bigint,
        COALESCE(c.reltuples, 0)::bigint
    ) AS live_rows,
    COALESCE(pg_total_relation_size(c.oid), 0)::bigint AS total_size_bytes
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid
WHERE n.nspname = 'public' AND c.relkind = 'r'
ORDER BY total_size_bytes DESC
"#;

#[derive(Clone)]
struct DbConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    dbname: String,
}

impl DbConfig {
    fn to_conn_string(&self) -> String {
        format!(
            "host={} port={} user={} password={} dbname={} connect_timeout=5",
            self.host, self.port, self.user, self.password, self.dbname
        )
    }
}

#[derive(Clone)]
struct AppState {
    db: DbConfig,
    logs: LogStores,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TableStats {
    table_name: String,
    live_rows: i64,
    total_size_bytes: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TableTotals {
    live_rows: i64,
    total_size_bytes: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusPayload {
    db_size_bytes: i64,
    connected_clients: i64,
    table_count: i64,
    uptime_seconds: i64,
    table_stats: Vec<TableStats>,
    table_totals: TableTotals,
    largest_table: Option<String>,
    timestamp: String,
}

#[derive(Serialize)]
struct ErrorPayload {
    error: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogResponse {
    lines: Vec<crate::runtime::services::log_store::LogLine>,
}

async fn collect_stats(db: &DbConfig) -> Result<StatusPayload> {
    let (client, connection) = tokio_postgres::connect(&db.to_conn_string(), NoTls)
        .await
        .map_err(|err| Error::Custom(err.to_string()))?;
    spawn(async move {
        if let Err(err) = connection.await {
            log_warn!("self-hosted-db: postgres connection error: {err}");
        }
        Ok(())
    });

    let size_row = client
        .query_one("SELECT pg_database_size(current_database())::bigint", &[])
        .await
        .map_err(|err| Error::Custom(err.to_string()))?;
    let db_size_bytes: i64 = size_row.get(0);

    let conn_row = client
        .query_one(
            "SELECT COUNT(*)::int FROM pg_stat_activity WHERE datname = current_database()",
            &[],
        )
        .await
        .map_err(|err| Error::Custom(err.to_string()))?;
    let connected_clients: i64 = conn_row.get::<_, i32>(0) as i64;

    let table_row = client
        .query_one(
            "SELECT COUNT(*)::int FROM information_schema.tables WHERE table_schema = 'public'",
            &[],
        )
        .await
        .map_err(|err| Error::Custom(err.to_string()))?;
    let table_count: i64 = table_row.get::<_, i32>(0) as i64;

    let uptime_row = client
        .query_one(
            "SELECT EXTRACT(EPOCH FROM now() - pg_postmaster_start_time())::int",
            &[],
        )
        .await
        .map_err(|err| Error::Custom(err.to_string()))?;
    let uptime_seconds: i64 = uptime_row.get::<_, i32>(0) as i64;

    let rows = client
        .query(TABLE_STATS_SQL, &[])
        .await
        .map_err(|err| Error::Custom(err.to_string()))?;
    let table_stats = rows
        .into_iter()
        .map(|row| TableStats {
            table_name: row
                .get::<_, Option<String>>(0)
                .unwrap_or_else(|| "unknown".to_string()),
            live_rows: row.get::<_, i64>(1),
            total_size_bytes: row.get::<_, i64>(2),
        })
        .collect::<Vec<_>>();

    let total_live_rows = table_stats.iter().map(|row| row.live_rows).sum();
    let total_size_bytes = table_stats.iter().map(|row| row.total_size_bytes).sum();
    let largest_table = table_stats.first().map(|row| row.table_name.clone());

    Ok(StatusPayload {
        db_size_bytes,
        connected_clients,
        table_count,
        uptime_seconds,
        table_stats,
        table_totals: TableTotals {
            live_rows: total_live_rows,
            total_size_bytes,
        },
        largest_table,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

async fn status_handler(State(state): State<AppState>) -> Response {
    match collect_stats(&state.db).await {
        Ok(payload) => Json(payload).into_response(),
        Err(err) => {
            log_warn!("self-hosted-db: status error: {err}");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorPayload {
                    error: "Unable to collect indexer metrics",
                }),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
struct StreamQuery {
    interval: Option<u64>,
}

async fn status_stream_handler(
    State(state): State<AppState>,
    Query(query): Query<StreamQuery>,
) -> Sse<impl futures::Stream<Item = std::result::Result<axum::response::sse::Event, Infallible>>> {
    let interval_secs = query.interval.map(|value| value.clamp(5, 60)).unwrap_or(10);

    let db = state.db.clone();
    let stream = IntervalStream::new(tokio::time::interval(Duration::from_secs(interval_secs)))
        .then(move |_| {
            let db = db.clone();
            async move {
                match collect_stats(&db).await {
                    Ok(payload) => Ok(axum::response::sse::Event::default()
                        .json_data(payload)
                        .unwrap()),
                    Err(_) => Ok(axum::response::sse::Event::default()
                        .event("error")
                        .json_data(ErrorPayload {
                            error: "Unable to collect indexer metrics",
                        })
                        .unwrap()),
                }
            }
        });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

#[derive(Deserialize)]
struct LogsQuery {
    limit: Option<usize>,
}

async fn logs_handler(
    State(state): State<AppState>,
    AxumPath(service): AxumPath<String>,
    Query(query): Query<LogsQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(10).clamp(10, 1000);
    let lines = match service.as_str() {
        "postgres" => state.logs.postgres.snapshot(limit),
        "indexer" => state.logs.indexer.snapshot(limit),
        "k-indexer" => state.logs.k_indexer.snapshot(limit),
        "rest" => state.logs.rest.snapshot(limit),
        "socket" => state.logs.socket.snapshot(limit),
        _ => {
            return (StatusCode::NOT_FOUND, "Unknown service").into_response();
        }
    };

    Json(LogResponse { lines }).into_response()
}

pub enum SelfHostedDbEvents {
    Enable,
    Disable,
    UpdateSettings(SelfHostedSettings),
    UpdateNodeSettings(NodeSettings),
    Exit,
}

struct ServerHandle {
    shutdown: Option<oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<()>,
}

pub struct SelfHostedDbService {
    pub application_events: ApplicationEventsChannel,
    pub service_events: Channel<SelfHostedDbEvents>,
    pub task_ctl: Channel<()>,
    pub settings: Mutex<SelfHostedSettings>,
    pub node_settings: Mutex<NodeSettings>,
    pub is_enabled: AtomicBool,
    logs: LogStores,
    server: Mutex<Option<ServerHandle>>,
}

impl SelfHostedDbService {
    pub fn new(
        application_events: ApplicationEventsChannel,
        settings: &Settings,
        logs: LogStores,
    ) -> Self {
        Self {
            application_events,
            service_events: Channel::unbounded(),
            task_ctl: Channel::oneshot(),
            settings: Mutex::new(settings.self_hosted.clone()),
            node_settings: Mutex::new(settings.node.clone()),
            is_enabled: AtomicBool::new(settings.self_hosted.enabled),
            logs,
            server: Mutex::new(None),
        }
    }

    pub fn enable(&self, enable: bool) {
        if enable {
            self.service_events
                .try_send(SelfHostedDbEvents::Enable)
                .unwrap();
        } else {
            self.service_events
                .try_send(SelfHostedDbEvents::Disable)
                .unwrap();
        }
    }

    pub fn update_settings(&self, settings: SelfHostedSettings) {
        self.service_events
            .try_send(SelfHostedDbEvents::UpdateSettings(settings))
            .unwrap();
    }

    pub fn update_node_settings(&self, settings: NodeSettings) {
        self.service_events
            .try_send(SelfHostedDbEvents::UpdateNodeSettings(settings))
            .unwrap();
    }

    async fn start_server(&self) -> Result<()> {
        if self.server.lock().unwrap().is_some() {
            return Ok(());
        }

        let settings = self.settings.lock().unwrap().clone();
        let node_settings = self.node_settings.lock().unwrap().clone();
        let api_port = settings.effective_api_port(node_settings.network);
        let db_port = settings.effective_db_port(node_settings.network);
        let addr = format!("{}:{}", settings.api_bind, api_port)
            .parse::<std::net::SocketAddr>()
            .map_err(|err| Error::Custom(format!("invalid bind address: {err}")))?;
        let db_name = crate::settings::self_hosted_db_name_for_network(
            &settings.db_name,
            node_settings.network,
        );

        let state = AppState {
            db: DbConfig {
                host: settings.db_host,
                port: db_port,
                user: settings.db_user,
                password: settings.db_password,
                dbname: db_name,
            },
            logs: self.logs.clone(),
        };

        let app = Router::new()
            .route("/api/status", get(status_handler))
            .route("/api/status/stream", get(status_stream_handler))
            .route("/api/healthz", get(health_handler))
            .route("/api/logs/:service", get(logs_handler))
            .with_state(state);

        let listener = match TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                return Err(Error::Custom(format!(
                    "address already in use ({}:{}): another process is already bound",
                    settings.api_bind, api_port
                )));
            }
            Err(err) => return Err(err.into()),
        };
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let join = tokio::spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            });
            if let Err(err) = server.await {
                log_warn!("self-hosted-db: server error: {err}");
            }
        });

        self.server.lock().unwrap().replace(ServerHandle {
            shutdown: Some(shutdown_tx),
            join,
        });

        Ok(())
    }

    async fn stop_server(&self) {
        let handle = self.server.lock().unwrap().take();
        if let Some(mut handle) = handle {
            if let Some(shutdown) = handle.shutdown.take() {
                let _ = shutdown.send(());
            }
            let _ = handle.join.await;
        }
    }
}

#[async_trait]
impl Service for SelfHostedDbService {
    fn name(&self) -> &'static str {
        "self-hosted-db"
    }

    async fn spawn(self: Arc<Self>) -> Result<()> {
        if self.is_enabled.load(Ordering::Relaxed) {
            if let Err(err) = self.start_server().await {
                log_warn!("self-hosted-db: failed to start server: {err}");
            }
        }

        loop {
            select! {
                msg = self.service_events.receiver.recv().fuse() => {
                    if let Ok(event) = msg {
                        match event {
                            SelfHostedDbEvents::Enable => {
                                let was_enabled = self.is_enabled.swap(true, Ordering::Relaxed);
                                if !was_enabled {
                                    if let Err(err) = self.start_server().await {
                                        log_warn!("self-hosted-db: failed to start server: {err}");
                                    }
                                }
                            }
                            SelfHostedDbEvents::Disable => {
                                let was_enabled = self.is_enabled.swap(false, Ordering::Relaxed);
                                if was_enabled {
                                    self.stop_server().await;
                                }
                            }
                            SelfHostedDbEvents::UpdateSettings(settings) => {
                                *self.settings.lock().unwrap() = settings;
                                if self.is_enabled.load(Ordering::Relaxed) {
                                    self.stop_server().await;
                                    if let Err(err) = self.start_server().await {
                                        log_warn!("self-hosted-db: failed to restart server: {err}");
                                    }
                                }
                            }
                            SelfHostedDbEvents::UpdateNodeSettings(settings) => {
                                *self.node_settings.lock().unwrap() = settings;
                                if self.is_enabled.load(Ordering::Relaxed) {
                                    self.stop_server().await;
                                    if let Err(err) = self.start_server().await {
                                        log_warn!("self-hosted-db: failed to restart server: {err}");
                                    }
                                }
                            }
                            SelfHostedDbEvents::Exit => {
                                break;
                            }
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        self.stop_server().await;
        self.task_ctl.send(()).await.unwrap();
        Ok(())
    }

    fn terminate(self: Arc<Self>) {
        let _ = self
            .service_events
            .sender
            .try_send(SelfHostedDbEvents::Exit);
    }

    async fn join(self: Arc<Self>) -> Result<()> {
        self.task_ctl.recv().await.unwrap();
        Ok(())
    }
}
