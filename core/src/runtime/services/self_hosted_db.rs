use crate::imports::*;
use crate::runtime::services::{
    LoaderStatusSnapshot, LogStores, SelfHostedKasiaIndexerService, SharedLoaderStatus,
};
use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    routing::get,
};
use serde::Serialize;
use std::convert::Infallible;
use std::path::Path;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_postgres::NoTls;
use tokio_stream::wrappers::IntervalStream;
use walkdir::WalkDir;

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
    loader_status: SharedLoaderStatus,
    kasia_metrics_url: String,
    kasia_partitions_root: PathBuf,
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
    error: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogResponse {
    lines: Vec<crate::runtime::services::log_store::LogLine>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
struct KasiaMetricsSnapshot {
    handshakes_by_sender: u64,
    uniq_handshakes_by_receiver: u64,
    payments_by_sender: u64,
    uniq_payments_by_receiver: u64,
    contextual_messages: u64,
    blocks_processed: u64,
    unknown_sender_entries: u64,
}

#[derive(Clone, Copy)]
struct KasiaTableEstimate {
    table_name: &'static str,
    row_estimate: Option<i64>,
}

const KASIA_TABLES: [KasiaTableEstimate; 17] = [
    KasiaTableEstimate {
        table_name: "metadata",
        row_estimate: Some(1),
    },
    KasiaTableEstimate {
        table_name: "block_compact_header",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "daa_index_compact_header",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "block_gaps",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "handshake_by_receiver",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "handshake_by_sender",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "tx-id-to-handshake",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "contextual_message_by_sender",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "payment_by_receiver",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "payment_by_sender",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "tx_id_to_payment",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "accepting_block_to_tx_id",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "tx_id_to_acceptance",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "pending_sender_resolution",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "self_stash_by_owner",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "tx-id-to-self-stash",
        row_estimate: None,
    },
    KasiaTableEstimate {
        table_name: "tx-id-to-contextual-message",
        row_estimate: None,
    },
];

async fn fetch_kasia_metrics(url: &str) -> Option<KasiaMetricsSnapshot> {
    let fut = http::get_json::<KasiaMetricsSnapshot>(url);
    match tokio::time::timeout(Duration::from_millis(1200), fut).await {
        Ok(Ok(metrics)) => Some(metrics),
        _ => None,
    }
}

fn estimate_kasia_rows(metrics: &KasiaMetricsSnapshot, table_name: &str) -> Option<i64> {
    let value = match table_name {
        "metadata" => 1,
        "block_compact_header" => metrics.blocks_processed,
        "daa_index_compact_header" => metrics.blocks_processed,
        "handshake_by_receiver" => metrics.uniq_handshakes_by_receiver,
        "handshake_by_sender" => metrics.handshakes_by_sender,
        "tx-id-to-handshake" => metrics.uniq_handshakes_by_receiver,
        "contextual_message_by_sender" => metrics.contextual_messages,
        "tx-id-to-contextual-message" => metrics.contextual_messages,
        "payment_by_receiver" => metrics.uniq_payments_by_receiver,
        "payment_by_sender" => metrics.payments_by_sender,
        "tx_id_to_payment" => metrics.uniq_payments_by_receiver,
        "pending_sender_resolution" => metrics.unknown_sender_entries,
        _ => return None,
    };
    Some(value as i64)
}

fn collect_kasia_partition_sizes(partitions_root: &Path) -> HashMap<String, i64> {
    let mut sizes = HashMap::<String, i64>::new();
    if !partitions_root.exists() {
        return sizes;
    }

    for entry in WalkDir::new(partitions_root).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel_path) = entry.path().strip_prefix(partitions_root) else {
            continue;
        };
        let Some(first_component) = rel_path.iter().next() else {
            continue;
        };
        let Some(partition_name) = first_component.to_str() else {
            continue;
        };
        let file_len = entry.metadata().map(|meta| meta.len() as i64).unwrap_or(0);
        *sizes.entry(partition_name.to_string()).or_insert(0) += file_len;
    }

    sizes
}

async fn collect_stats(state: &AppState) -> Result<StatusPayload> {
    let db = &state.db;
    let (client, connection) = tokio_postgres::connect(&db.to_conn_string(), NoTls)
        .await
        .map_err(|err| {
            let raw = err.to_string();
            let lower = raw.to_ascii_lowercase();
            if lower.contains("error connecting to server")
                || lower.contains("connection refused")
                || lower.contains("timed out")
            {
                Error::Custom(format!(
                    "database not ready: failed to connect to postgres at {}:{} ({raw})",
                    db.host, db.port
                ))
            } else {
                Error::Custom(raw)
            }
        })?;
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
    let mut table_stats = rows
        .into_iter()
        .map(|row| TableStats {
            table_name: row
                .get::<_, Option<String>>(0)
                .unwrap_or_else(|| "unknown".to_string()),
            live_rows: row.get::<_, i64>(1),
            total_size_bytes: row.get::<_, i64>(2),
        })
        .collect::<Vec<_>>();

    let kasia_metrics = fetch_kasia_metrics(&state.kasia_metrics_url).await;
    let kasia_sizes = tokio::task::spawn_blocking({
        let root = state.kasia_partitions_root.clone();
        move || collect_kasia_partition_sizes(&root)
    })
    .await
    .unwrap_or_default();
    let kasia_total_size_bytes: i64 = kasia_sizes.values().copied().sum();

    let mut existing_names = table_stats
        .iter()
        .map(|stats| stats.table_name.clone())
        .collect::<std::collections::HashSet<String>>();

    for estimate in KASIA_TABLES {
        if existing_names.contains(estimate.table_name) {
            continue;
        }
        let live_rows = kasia_metrics
            .as_ref()
            .and_then(|metrics| estimate_kasia_rows(metrics, estimate.table_name))
            .or(estimate.row_estimate)
            .unwrap_or(0);
        let total_size_bytes = kasia_sizes.get(estimate.table_name).copied().unwrap_or(0);
        table_stats.push(TableStats {
            table_name: estimate.table_name.to_string(),
            live_rows,
            total_size_bytes,
        });
        existing_names.insert(estimate.table_name.to_string());
    }

    let total_live_rows = table_stats.iter().map(|row| row.live_rows).sum();
    let total_size_bytes = table_stats.iter().map(|row| row.total_size_bytes).sum();
    let largest_table = table_stats.first().map(|row| row.table_name.clone());

    Ok(StatusPayload {
        db_size_bytes: db_size_bytes.saturating_add(kasia_total_size_bytes),
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
    match collect_stats(&state).await {
        Ok(payload) => Json(payload).into_response(),
        Err(err) => {
            let error_message = err.to_string();
            log_warn!("self-hosted-db: status error: {}", error_message);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorPayload {
                    error: error_message,
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

    let app_state = state.clone();
    let stream = IntervalStream::new(tokio::time::interval(Duration::from_secs(interval_secs)))
        .then(move |_| {
            let app_state = app_state.clone();
            async move {
                match collect_stats(&app_state).await {
                    Ok(payload) => Ok(axum::response::sse::Event::default()
                        .json_data(payload)
                        .unwrap()),
                    Err(err) => Ok(axum::response::sse::Event::default()
                        .event("error")
                        .json_data(ErrorPayload {
                            error: err.to_string(),
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

async fn loader_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let snapshot: LoaderStatusSnapshot = state.loader_status.snapshot();
    Json(snapshot)
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
        "loader" => state.logs.loader.snapshot(limit),
        "postgres" => state.logs.postgres.snapshot(limit),
        "indexer" => state.logs.indexer.snapshot(limit),
        "k-indexer" => state.logs.k_indexer.snapshot(limit),
        "kasia-indexer" => state.logs.kasia_indexer.snapshot(limit),
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
    loader_status: SharedLoaderStatus,
    server: Mutex<Option<ServerHandle>>,
}

impl SelfHostedDbService {
    fn resolve_api_host(bind: &str) -> String {
        let trimmed = bind.trim();
        if trimmed.is_empty() || trimmed == "0.0.0.0" || trimmed == "::" || trimmed == "[::]" {
            "127.0.0.1".to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn new(
        application_events: ApplicationEventsChannel,
        settings: &Settings,
        logs: LogStores,
        loader_status: SharedLoaderStatus,
    ) -> Self {
        Self {
            application_events,
            service_events: Channel::unbounded(),
            task_ctl: Channel::oneshot(),
            settings: Mutex::new(settings.self_hosted.clone()),
            node_settings: Mutex::new(settings.node.clone()),
            is_enabled: AtomicBool::new(false),
            logs,
            loader_status,
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
        let api_host = Self::resolve_api_host(&settings.api_bind);
        let kasia_ports =
            SelfHostedKasiaIndexerService::health_probe_ports(&settings, &node_settings);
        let kasia_metrics_port = kasia_ports
            .iter()
            .copied()
            .find(|port| *port == SelfHostedKasiaIndexerService::RUNTIME_API_PORT)
            .unwrap_or_else(|| settings.effective_kasia_indexer_port(node_settings.network));
        let kasia_metrics_url = format!("http://{}:{}/metrics", api_host, kasia_metrics_port);
        let kasia_partitions_root = workflow_core::dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".kasia-indexer")
            .join(node_settings.network.to_string())
            .join("partitions");

        let state = AppState {
            db: DbConfig {
                host: settings.db_host,
                port: db_port,
                user: settings.db_user,
                password: settings.db_password,
                dbname: db_name,
            },
            logs: self.logs.clone(),
            loader_status: self.loader_status.clone(),
            kasia_metrics_url,
            kasia_partitions_root,
        };

        let app = Router::new()
            .route("/api/status", get(status_handler))
            .route("/api/status/stream", get(status_stream_handler))
            .route("/api/healthz", get(health_handler))
            .route("/api/loader-status", get(loader_status_handler))
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
