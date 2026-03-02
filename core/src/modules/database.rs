use crate::imports::*;
use kaspa_metrics_core::data::as_data_size;

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TableStats {
    table_name: String,
    live_rows: i64,
    total_size_bytes: i64,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogLine {
    ts: String,
    level: String,
    message: String,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogResponse {
    lines: Vec<LogLine>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoaderStatus {
    phase: String,
    message: String,
    connected: bool,
    postgres_ready: bool,
    indexers_ready: bool,
    rest_ready: bool,
    socket_ready: bool,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DatabaseStatus {
    db_size_bytes: i64,
    connected_clients: i64,
    table_count: i64,
    table_stats: Vec<TableStats>,
}

#[derive(Default)]
struct DatabaseState {
    status: Option<DatabaseStatus>,
    loader_status: Option<LoaderStatus>,
    last_error: Option<String>,
    last_updated: Option<Instant>,
    in_flight: bool,
    loader_last_error: Option<String>,
    loader_last_updated: Option<Instant>,
    loader_in_flight: bool,
    logs_loader: Vec<LogLine>,
    logs_postgres: Vec<LogLine>,
    logs_indexer: Vec<LogLine>,
    logs_k_indexer: Vec<LogLine>,
    logs_kasia_indexer: Vec<LogLine>,
    logs_rest: Vec<LogLine>,
    logs_socket: Vec<LogLine>,
    logs_last_error: Option<String>,
    logs_last_updated: Option<Instant>,
    logs_in_flight: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LogService {
    Loader,
    Postgres,
    Indexer,
    KIndexer,
    KasiaIndexer,
    Rest,
    Socket,
}

pub struct Database {
    #[allow(dead_code)]
    runtime: Runtime,
    state: Arc<Mutex<DatabaseState>>,
    poll_interval: Duration,
    log_poll_interval: Duration,
    log_service: LogService,
    log_autoscroll: bool,
    log_limit: usize,
}

impl Database {
    const API_TIMEOUT: Duration = Duration::from_secs(2);

    pub fn new(runtime: Runtime) -> Self {
        Self {
            runtime,
            state: Arc::new(Mutex::new(DatabaseState::default())),
            poll_interval: Duration::from_secs(8),
            log_poll_interval: Duration::from_secs(2),
            log_service: LogService::Loader,
            log_autoscroll: true,
            log_limit: 1000,
        }
    }

    fn resolve_api_host(bind: &str) -> String {
        let trimmed = bind.trim();
        if trimmed.is_empty() || trimmed == "0.0.0.0" || trimmed == "::" || trimmed == "[::]" {
            "127.0.0.1".to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn api_url(settings: &SelfHostedSettings, network: Network) -> String {
        let host = Self::resolve_api_host(&settings.api_bind);
        format!(
            "http://{}:{}/api/status",
            host,
            settings.effective_api_port(network)
        )
    }

    #[cfg(target_arch = "wasm32")]
    fn loader_url(settings: &SelfHostedSettings, network: Network) -> String {
        let host = Self::resolve_api_host(&settings.api_bind);
        format!(
            "http://{}:{}/api/loader-status",
            host,
            settings.effective_api_port(network)
        )
    }

    fn loader_log_lines_from_runtime(limit: usize) -> Vec<LogLine> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            runtime()
                .self_hosted_loader_service()
                .log_snapshot(limit)
                .into_iter()
                .map(|line| LogLine {
                    ts: line.ts,
                    level: line.level,
                    message: line.message,
                })
                .collect()
        }

        #[cfg(target_arch = "wasm32")]
        {
            let _ = limit;
            Vec::new()
        }
    }

    fn logs_url(
        settings: &SelfHostedSettings,
        network: Network,
        service: LogService,
        limit: usize,
    ) -> String {
        let host = Self::resolve_api_host(&settings.api_bind);
        let service = match service {
            LogService::Loader => "loader",
            LogService::Postgres => "postgres",
            LogService::Indexer => "indexer",
            LogService::KIndexer => "k-indexer",
            LogService::KasiaIndexer => "kasia-indexer",
            LogService::Rest => "rest",
            LogService::Socket => "socket",
        };
        format!(
            "http://{}:{}/api/logs/{}?limit={}",
            host,
            settings.effective_api_port(network),
            service,
            limit
        )
    }

    fn schedule_fetch(&self, settings: &SelfHostedSettings, network: Network) {
        let should_fetch = {
            let mut state = self.state.lock().unwrap();
            if state.in_flight {
                false
            } else {
                let elapsed = state
                    .last_updated
                    .map(|time| time.elapsed() >= self.poll_interval)
                    .unwrap_or(true);
                if elapsed {
                    state.in_flight = true;
                    true
                } else {
                    false
                }
            }
        };

        if !should_fetch {
            return;
        }

        let url = Self::api_url(settings, network);
        let settings_snapshot = settings.clone();
        let state = self.state.clone();
        spawn(async move {
            let result: std::result::Result<DatabaseStatus, String> =
                match tokio::time::timeout(Self::API_TIMEOUT, http::get_json::<DatabaseStatus>(&url)).await {
                Ok(inner) => inner.map_err(|err| err.to_string()),
                Err(_) => Err("database status request timed out".to_string()),
            };
            let (status_opt, error_opt) = match result {
                Ok(status) => (Some(status), None),
                Err(http_err) => match Self::direct_db_status(&settings_snapshot, network).await {
                    Ok(status) => (Some(status), None),
                    Err(db_err) => (None, Some(format!("api: {}; direct-db: {}", http_err, db_err))),
                },
            };
            {
                let mut guard = state.lock().unwrap();
                guard.in_flight = false;
                guard.last_updated = Some(Instant::now());
                if let Some(status) = status_opt {
                    guard.status = Some(status);
                    guard.last_error = None;
                } else {
                    guard.last_error = error_opt;
                    if guard.status.is_none() {
                        guard.status = Some(DatabaseStatus::default());
                    }
                }
            }
            runtime().request_repaint();
            Ok(())
        });
    }

    async fn direct_db_status(
        settings: &SelfHostedSettings,
        network: Network,
    ) -> std::result::Result<DatabaseStatus, String> {
        let db_name = crate::settings::self_hosted_db_name_for_network(&settings.db_name, network);
        let conn = format!(
            "host={} port={} user={} password={} dbname={} connect_timeout=3",
            settings.db_host,
            settings.effective_db_port(network),
            settings.db_user,
            settings.db_password,
            db_name
        );

        let (client, connection) = match tokio_postgres::connect(&conn, tokio_postgres::NoTls).await
        {
            Ok(v) => v,
            Err(err) => {
                let raw = err.to_string();
                let lower = raw.to_ascii_lowercase();
                if lower.contains("does not exist") && lower.contains("database") {
                    return Ok(DatabaseStatus {
                        db_size_bytes: 0,
                        connected_clients: 0,
                        table_count: 0,
                        table_stats: Vec::new(),
                    });
                }
                return Err(raw);
            }
        };

        spawn(async move {
            let _ = connection.await;
            Ok(())
        });

        let size_row = client
            .query_one("SELECT pg_database_size(current_database())::bigint", &[])
            .await
            .map_err(|err| err.to_string())?;
        let conn_row = client
            .query_one(
                "SELECT COUNT(*)::int FROM pg_stat_activity WHERE datname = current_database()",
                &[],
            )
            .await
            .map_err(|err| err.to_string())?;
        let table_row = client
            .query_one(
                "SELECT COUNT(*)::int FROM information_schema.tables WHERE table_schema = 'public'",
                &[],
            )
            .await
            .map_err(|err| err.to_string())?;

        Ok(DatabaseStatus {
            db_size_bytes: size_row.get::<_, i64>(0),
            connected_clients: conn_row.get::<_, i32>(0) as i64,
            table_count: table_row.get::<_, i32>(0) as i64,
            table_stats: Vec::new(),
        })
    }

    fn schedule_loader_fetch(&self, settings: &SelfHostedSettings, network: Network) {
        let should_fetch = {
            let mut state = self.state.lock().unwrap();
            if state.loader_in_flight {
                false
            } else {
                let elapsed = state
                    .loader_last_updated
                    .map(|time| time.elapsed() >= self.log_poll_interval)
                    .unwrap_or(true);
                if elapsed {
                    state.loader_in_flight = true;
                    true
                } else {
                    false
                }
            }
        };

        if !should_fetch {
            return;
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (settings, network);
            let snapshot = runtime().self_hosted_loader_service().status_snapshot();
            let mut guard = self.state.lock().unwrap();
            guard.loader_in_flight = false;
            guard.loader_last_updated = Some(Instant::now());
            guard.loader_status = Some(LoaderStatus {
                phase: snapshot.phase,
                message: snapshot.message,
                connected: snapshot.connected,
                postgres_ready: snapshot.postgres_ready,
                indexers_ready: snapshot.indexers_ready,
                rest_ready: snapshot.rest_ready,
                socket_ready: snapshot.socket_ready,
            });
            guard.loader_last_error = None;
        }

        #[cfg(target_arch = "wasm32")]
        {
            let url = Self::loader_url(settings, network);
            let state = self.state.clone();
            spawn(async move {
                let result: std::result::Result<LoaderStatus, String> = match tokio::time::timeout(
                    Self::API_TIMEOUT,
                    http::get_json::<LoaderStatus>(&url),
                )
                .await
                {
                    Ok(inner) => inner.map_err(|err| err.to_string()),
                    Err(_) => Err("loader status request timed out".to_string()),
                };
                let mut guard = state.lock().unwrap();
                guard.loader_in_flight = false;
                guard.loader_last_updated = Some(Instant::now());
                match result {
                    Ok(status) => {
                        guard.loader_status = Some(status);
                        guard.loader_last_error = None;
                    }
                    Err(err) => {
                        guard.loader_last_error = Some(err);
                    }
                }
                runtime().request_repaint();
                Ok(())
            });
        }
    }

    fn schedule_logs_fetch(&self, settings: &SelfHostedSettings, network: Network, service: LogService) {
        if matches!(service, LogService::Loader) {
            let lines = Self::loader_log_lines_from_runtime(self.log_limit);
            let mut guard = self.state.lock().unwrap();
            guard.logs_loader = lines;
            guard.logs_last_error = None;
            guard.logs_last_updated = Some(Instant::now());
            guard.logs_in_flight = false;
            return;
        }

        let should_fetch = {
            let mut state = self.state.lock().unwrap();
            if state.logs_in_flight {
                false
            } else {
                let elapsed = state
                    .logs_last_updated
                    .map(|time| time.elapsed() >= self.log_poll_interval)
                    .unwrap_or(true);
                if elapsed {
                    state.logs_in_flight = true;
                    true
                } else {
                    false
                }
            }
        };

        if !should_fetch {
            return;
        }

        let url = Self::logs_url(settings, network, service, self.log_limit);
        let state = self.state.clone();
        spawn(async move {
            let result: std::result::Result<LogResponse, String> =
                match tokio::time::timeout(Self::API_TIMEOUT, http::get_json::<LogResponse>(&url)).await {
                Ok(inner) => inner.map_err(|err| err.to_string()),
                Err(_) => Err("logs request timed out".to_string()),
            };
            let mut guard = state.lock().unwrap();
            guard.logs_in_flight = false;
            guard.logs_last_updated = Some(Instant::now());
            match result {
                Ok(response) => {
                    match service {
                        LogService::Loader => guard.logs_loader = response.lines,
                        LogService::Postgres => guard.logs_postgres = response.lines,
                        LogService::Indexer => guard.logs_indexer = response.lines,
                        LogService::KIndexer => guard.logs_k_indexer = response.lines,
                        LogService::KasiaIndexer => guard.logs_kasia_indexer = response.lines,
                        LogService::Rest => guard.logs_rest = response.lines,
                        LogService::Socket => guard.logs_socket = response.lines,
                    }
                    guard.logs_last_error = None;
                }
                Err(err) => {
                    guard.logs_last_error = Some(err);
                }
            }
            runtime().request_repaint();
            Ok(())
        });
    }

    fn format_bytes(bytes: i64) -> String {
        if bytes <= 0 {
            return "0 B".to_string();
        }
        as_data_size(bytes as f64, true)
    }

    fn format_count(value: i64) -> String {
        if value >= 1_000_000 {
            format!("{:.1}M", value as f64 / 1_000_000.0)
        } else if value >= 1_000 {
            format!("{:.1}k", value as f64 / 1_000.0)
        } else {
            value.to_string()
        }
    }

    fn render_disabled(&self, _core: &mut Core, ui: &mut Ui) {
        let fill = theme_color().kaspa_color.linear_multiply(0.08);
        let stroke = Stroke::new(1.0, theme_color().kaspa_color.linear_multiply(0.5));
        Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(16))
            .inner_margin(egui::Margin::symmetric(24, 20))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            egui_phosphor::light::DATABASE,
                            i18n("Database")
                        ))
                        .size(28.0)
                        .strong()
                        .color(theme_color().strong_color),
                    );
                    ui.add_space(8.0);
                    ui.label(i18n(
                        "Self-hosted database services are disabled. Enable them in Settings to unlock metrics and status views.",
                    ));
                    ui.add_space(12.0);
                    ui.label(i18n("Enable Self Hosted in Settings to use this view."));
                });
            });
    }
}

impl ModuleT for Database {
    fn name(&self) -> Option<&'static str> {
        Some(i18n("Database"))
    }

    fn render(
        &mut self,
        core: &mut Core,
        _ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        ui: &mut egui::Ui,
    ) {
        if !core.settings.self_hosted.enabled {
            self.render_disabled(core, ui);
            return;
        }

        let network = core.settings.node.network;
        self.schedule_fetch(&core.settings.self_hosted, network);
        self.schedule_loader_fetch(&core.settings.self_hosted, network);
        self.schedule_logs_fetch(&core.settings.self_hosted, network, self.log_service);

        let (status, error, mut loader_status, loader_error) = {
            let state = self.state.lock().unwrap();
            (
                state.status.clone(),
                state.last_error.clone(),
                state.loader_status.clone(),
                state.loader_last_error.clone(),
            )
        };

        #[cfg(not(target_arch = "wasm32"))]
        if loader_status.is_none() {
            let snapshot = runtime().self_hosted_loader_service().status_snapshot();
            loader_status = Some(LoaderStatus {
                phase: snapshot.phase,
                message: snapshot.message,
                connected: snapshot.connected,
                postgres_ready: snapshot.postgres_ready,
                indexers_ready: snapshot.indexers_ready,
                rest_ready: snapshot.rest_ready,
                socket_ready: snapshot.socket_ready,
            });
        }

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.heading(i18n("Database"));
                    ui.separator();

                    if let Some(loader_error) = &loader_error {
                        ui.label(
                            RichText::new(loader_error)
                                .color(theme_color().warning_color)
                                .size(12.0),
                        );
                    } else if let Some(error) = &error {
                        let show_error = loader_status
                            .as_ref()
                            .map(|status| status.connected || status.postgres_ready)
                            .unwrap_or(true);
                        if show_error {
                        ui.label(
                            RichText::new(error)
                                .color(theme_color().warning_color)
                                .size(12.0),
                        );
                        }
                    }

                    ui.add_space(8.0);

                    CollapsingHeader::new(i18n("Overview"))
                        .default_open(true)
                        .show(ui, |ui| {
                            let value_color = theme_color().node_data_color;
                            let loader_connected = loader_status
                                .as_ref()
                                .map(|value| value.connected)
                                .unwrap_or(false);
                            let loader_phase = loader_status
                                .as_ref()
                                .map(|value| value.phase.as_str())
                                .unwrap_or("Initialisation");
                            let loader_message = loader_status
                                .as_ref()
                                .map(|value| value.message.clone())
                                .unwrap_or_else(|| "Waiting for Loader".to_string());
                            let (db_size, tables, clients) = if let Some(status) = &status {
                                (
                                    Self::format_bytes(status.db_size_bytes),
                                    status.table_count.to_string(),
                                    status.connected_clients.to_string(),
                                )
                            } else {
                                ("--".to_string(), "--".to_string(), "--".to_string())
                            };

                            Grid::new("db_overview_grid")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label(i18n("Status"));
                                    ui.horizontal(|ui| {
                                        if loader_connected {
                                            ui.colored_label(Color32::from_rgb(80, 200, 120), "●");
                                            ui.colored_label(value_color, i18n("Connected"));
                                        } else {
                                            ui.add(egui::Spinner::new().size(14.0));
                                            let status_label = i18n("Initialisation").to_string();
                                            ui.colored_label(value_color, status_label);
                                        }
                                    });
                                    ui.end_row();

                                    ui.label(i18n("Loader"));
                                    ui.colored_label(
                                        value_color,
                                        format!("{loader_phase}: {loader_message}"),
                                    );
                                    ui.end_row();

                                    ui.label(i18n("Database Size"));
                                    ui.colored_label(value_color, db_size);
                                    ui.end_row();

                                    ui.label(i18n("Tables"));
                                    ui.colored_label(value_color, tables);
                                    ui.end_row();

                            ui.label(i18n("Connections"));
                            ui.colored_label(value_color, clients);
                            ui.end_row();

                            if let Some(loader) = &loader_status {
                                ui.label(i18n("Checks"));
                                ui.colored_label(
                                    value_color,
                                    format!(
                                        "postgres={} indexers={} rest={} socket={}",
                                        if loader.postgres_ready { "ok" } else { "waiting" },
                                        if loader.indexers_ready { "ok" } else { "waiting" },
                                        if loader.rest_ready { "ok" } else { "waiting" },
                                        if loader.socket_ready { "ok" } else { "waiting" }
                                    ),
                                );
                                ui.end_row();

                            }
                        });
                });

            ui.add_space(8.0);

            CollapsingHeader::new(i18n("Indexer Tables"))
                .default_open(true)
                .show(ui, |ui| {
                    if let Some(status) = &status {
                        let mut table_rows = status.table_stats.clone();
                        table_rows.sort_by(|a, b| b.total_size_bytes.cmp(&a.total_size_bytes));

                        Grid::new("db_table_stats")
                            .striped(true)
                            .min_col_width(80.0)
                            .show(ui, |ui| {
                                ui.label(RichText::new(i18n("Table")).strong());
                                ui.label(RichText::new(i18n("Rows")).strong());
                                ui.label(RichText::new(i18n("Size")).strong());
                                ui.end_row();

                                for stats in table_rows {
                                    ui.label(stats.table_name);
                                    ui.label(Self::format_count(stats.live_rows));
                                    ui.label(Self::format_bytes(stats.total_size_bytes));
                                    ui.end_row();
                                }
                            });
                    } else {
                        ui.label(i18n("Awaiting metrics from the local API."));
                    }
                });

            ui.add_space(12.0);
            CollapsingHeader::new(i18n("Logs"))
                .default_open(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(i18n("Service"));
                        ComboBox::from_id_salt("db_logs_service")
                            .selected_text(match self.log_service {
                                LogService::Loader => i18n("Loader"),
                                LogService::Postgres => i18n("Postgres"),
                                LogService::Indexer => i18n("Indexer"),
                                LogService::KIndexer => i18n("K-indexer"),
                                LogService::KasiaIndexer => i18n("Kasia-indexer"),
                                LogService::Rest => i18n("REST API"),
                                LogService::Socket => i18n("Socket"),
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.log_service,
                                    LogService::Loader,
                                    i18n("Loader"),
                                );
                                ui.selectable_value(
                                    &mut self.log_service,
                                    LogService::Postgres,
                                    i18n("Postgres"),
                                );
                                ui.selectable_value(
                                    &mut self.log_service,
                                    LogService::Indexer,
                                    i18n("Indexer"),
                                );
                                ui.selectable_value(
                                    &mut self.log_service,
                                    LogService::KIndexer,
                                    i18n("K-indexer"),
                                );
                                ui.selectable_value(
                                    &mut self.log_service,
                                    LogService::KasiaIndexer,
                                    i18n("Kasia-indexer"),
                                );
                                ui.selectable_value(
                                    &mut self.log_service,
                                    LogService::Rest,
                                    i18n("REST API"),
                                );
                                ui.selectable_value(
                                    &mut self.log_service,
                                    LogService::Socket,
                                    i18n("Socket"),
                                );
                            });
                        ui.checkbox(&mut self.log_autoscroll, i18n("Autoscroll"));
                        if ui.small_button(i18n("Copy Logs")).clicked() {
                            let state = self.state.lock().unwrap();
                            let lines = match self.log_service {
                                LogService::Loader => &state.logs_loader,
                                LogService::Postgres => &state.logs_postgres,
                                LogService::Indexer => &state.logs_indexer,
                                LogService::KIndexer => &state.logs_k_indexer,
                                LogService::KasiaIndexer => &state.logs_kasia_indexer,
                                LogService::Rest => &state.logs_rest,
                                LogService::Socket => &state.logs_socket,
                            };
                            let mut out = String::new();
                            for line in lines {
                                out.push_str(&format!(
                                    "{} [{}] {}\n",
                                    line.ts, line.level, line.message
                                ));
                            }
                            ui.ctx().copy_text(out);
                            runtime().notify_clipboard(i18n("Copied to clipboard"));
                        }
                    });

                    let (lines, log_error) = {
                        let state = self.state.lock().unwrap();
                        let lines = match self.log_service {
                            LogService::Loader => state.logs_loader.clone(),
                            LogService::Postgres => state.logs_postgres.clone(),
                            LogService::Indexer => state.logs_indexer.clone(),
                            LogService::KIndexer => state.logs_k_indexer.clone(),
                            LogService::KasiaIndexer => state.logs_kasia_indexer.clone(),
                            LogService::Rest => state.logs_rest.clone(),
                            LogService::Socket => state.logs_socket.clone(),
                        };
                        (lines, state.logs_last_error.clone())
                    };

                    if let Some(log_error) = log_error {
                        ui.label(
                            RichText::new(log_error)
                                .color(theme_color().warning_color)
                                .size(12.0),
                        );
                    }

                    let row_height = ui.fonts(|fonts| {
                        let font_id = TextStyle::Monospace.resolve(ui.style());
                        fonts.row_height(&font_id)
                    });
                    let log_height =
                        row_height * 20.0 + ui.spacing().item_spacing.y * 2.0 + 8.0;

                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width(), log_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                        ScrollArea::vertical()
                            .auto_shrink([false; 2])
                            .stick_to_bottom(self.log_autoscroll)
                            .show(ui, |ui| {
                                if lines.is_empty() {
                                    ui.label(i18n("No logs yet."));
                                } else {
                                    for line in lines {
                                        let color = match line.level.as_str() {
                                            "ERROR" => theme_color().nack_color,
                                            "WARN" => theme_color().warning_color,
                                            _ => theme_color().node_data_color,
                                        };
                                        let text = format!(
                                            "{} [{}] {}",
                                            line.ts, line.level, line.message
                                        );
                                        ui.label(RichText::new(text).monospace().color(color));
                                    }
                                }
                            });
                        },
                    );
                });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(i18n("Loader manages startup order and health checks."));
                    });
                });
            });
    }
}
