use crate::imports::*;
use kaspa_metrics_core::data::as_data_size;
use std::collections::HashMap;

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
struct DatabaseStatus {
    db_size_bytes: i64,
    connected_clients: i64,
    table_count: i64,
    table_stats: Vec<TableStats>,
}

#[derive(Default)]
struct DatabaseState {
    status: Option<DatabaseStatus>,
    last_error: Option<String>,
    last_updated: Option<Instant>,
    in_flight: bool,
    logs_postgres: Vec<LogLine>,
    logs_indexer: Vec<LogLine>,
    logs_k_indexer: Vec<LogLine>,
    logs_rest: Vec<LogLine>,
    logs_socket: Vec<LogLine>,
    logs_last_error: Option<String>,
    logs_last_updated: Option<Instant>,
    logs_in_flight: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LogService {
    Postgres,
    Indexer,
    KIndexer,
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
    const INDEXER_TABLES: [&'static str; 19] = [
        "vars",
        "blocks",
        "block_parent",
        "subnetworks",
        "transactions",
        "transactions_acceptances",
        "blocks_transactions",
        "transactions_inputs",
        "transactions_outputs",
        "addresses_transactions",
        "scripts_transactions",
        "k_vars",
        "k_broadcasts",
        "k_votes",
        "k_mentions",
        "k_blocks",
        "k_follows",
        "k_contents",
        "k_hashtags",
    ];

    pub fn new(runtime: Runtime) -> Self {
        Self {
            runtime,
            state: Arc::new(Mutex::new(DatabaseState::default())),
            poll_interval: Duration::from_secs(8),
            log_poll_interval: Duration::from_secs(2),
            log_service: LogService::Postgres,
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

    fn api_url(settings: &SelfHostedSettings) -> String {
        let host = Self::resolve_api_host(&settings.api_bind);
        format!("http://{}:{}/api/status", host, settings.api_port)
    }

    fn logs_url(settings: &SelfHostedSettings, service: LogService, limit: usize) -> String {
        let host = Self::resolve_api_host(&settings.api_bind);
        let service = match service {
            LogService::Postgres => "postgres",
            LogService::Indexer => "indexer",
            LogService::KIndexer => "k-indexer",
            LogService::Rest => "rest",
            LogService::Socket => "socket",
        };
        format!(
            "http://{}:{}/api/logs/{}?limit={}",
            host, settings.api_port, service, limit
        )
    }

    fn schedule_fetch(&self, settings: &SelfHostedSettings) {
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

        let url = Self::api_url(settings);
        let state = self.state.clone();
        spawn(async move {
            let result = http::get_json::<DatabaseStatus>(&url).await;
            let mut guard = state.lock().unwrap();
            guard.in_flight = false;
            guard.last_updated = Some(Instant::now());
            match result {
                Ok(status) => {
                    guard.status = Some(status);
                    guard.last_error = None;
                }
                Err(err) => {
                    guard.last_error = Some(err.to_string());
                }
            }
            runtime().request_repaint();
            Ok(())
        });
    }

    fn schedule_logs_fetch(&self, settings: &SelfHostedSettings, service: LogService) {
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

        let url = Self::logs_url(settings, service, self.log_limit);
        let state = self.state.clone();
        spawn(async move {
            let result = http::get_json::<LogResponse>(&url).await;
            let mut guard = state.lock().unwrap();
            guard.logs_in_flight = false;
            guard.logs_last_updated = Some(Instant::now());
            match result {
                Ok(response) => {
                    match service {
                        LogService::Postgres => guard.logs_postgres = response.lines,
                        LogService::Indexer => guard.logs_indexer = response.lines,
                        LogService::KIndexer => guard.logs_k_indexer = response.lines,
                        LogService::Rest => guard.logs_rest = response.lines,
                        LogService::Socket => guard.logs_socket = response.lines,
                    }
                    guard.logs_last_error = None;
                }
                Err(err) => {
                    guard.logs_last_error = Some(err.to_string());
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

    fn is_indexer_initializing_error(error: &str) -> bool {
        let lower = error.to_ascii_lowercase();
        lower.contains("503")
            && lower.contains("unable to collect indexer metrics")
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

        self.schedule_fetch(&core.settings.self_hosted);
        self.schedule_logs_fetch(&core.settings.self_hosted, self.log_service);

        let (status, error) = {
            let state = self.state.lock().unwrap();
            (state.status.clone(), state.last_error.clone())
        };

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.heading(i18n("Database"));
                    ui.separator();

                    if let Some(error) = &error {
                        if Self::is_indexer_initializing_error(error) {
                            ui.label(
                                RichText::new(i18n(
                                    "Indexer metrics are still initializing. Database startup is in progress.",
                                ))
                                .color(theme_color().node_data_color)
                                .size(12.0),
                            );
                        } else {
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
                            let healthy = status.is_some() && error.is_none();
                            let connection_text = if healthy { "Connected" } else { "Offline" };
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
                                    ui.colored_label(value_color, connection_text);
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
                        });
                });

            ui.add_space(8.0);

            CollapsingHeader::new(i18n("Indexer Tables"))
                .default_open(true)
                .show(ui, |ui| {
                    if let Some(status) = &status {
                        let mut table_map: HashMap<&str, &TableStats> = HashMap::new();
                        for table in status.table_stats.iter() {
                            table_map.insert(table.table_name.as_str(), table);
                        }
                        let mut table_rows: Vec<(&str, Option<&TableStats>)> = Self::INDEXER_TABLES
                            .iter()
                            .map(|name| (*name, table_map.get(*name).copied()))
                            .collect();
                        table_rows.sort_by(|a, b| {
                            let a_size = a.1.map(|stats| stats.total_size_bytes).unwrap_or(-1);
                            let b_size = b.1.map(|stats| stats.total_size_bytes).unwrap_or(-1);
                            b_size.cmp(&a_size)
                        });

                        Grid::new("db_table_stats")
                            .striped(true)
                            .min_col_width(80.0)
                            .show(ui, |ui| {
                                ui.label(RichText::new(i18n("Table")).strong());
                                ui.label(RichText::new(i18n("Rows")).strong());
                                ui.label(RichText::new(i18n("Size")).strong());
                                ui.end_row();

                                for (table_name, stats) in table_rows {
                                    if let Some(stats) = stats {
                                        ui.label(table_name);
                                        ui.label(Self::format_count(stats.live_rows));
                                        ui.label(Self::format_bytes(stats.total_size_bytes));
                                    } else {
                                        ui.label(table_name);
                                        ui.label("--");
                                        ui.label("--");
                                    }
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
                                LogService::Postgres => i18n("Postgres"),
                                LogService::Indexer => i18n("Indexer"),
                                LogService::KIndexer => i18n("K-indexer"),
                                LogService::Rest => i18n("REST API"),
                                LogService::Socket => i18n("Socket"),
                            })
                            .show_ui(ui, |ui| {
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
                                LogService::Postgres => &state.logs_postgres,
                                LogService::Indexer => &state.logs_indexer,
                                LogService::KIndexer => &state.logs_k_indexer,
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
                            LogService::Postgres => state.logs_postgres.clone(),
                            LogService::Indexer => state.logs_indexer.clone(),
                            LogService::KIndexer => state.logs_k_indexer.clone(),
                            LogService::Rest => state.logs_rest.clone(),
                            LogService::Socket => state.logs_socket.clone(),
                        };
                        (lines, state.logs_last_error.clone())
                    };

                    if let Some(log_error) = log_error {
                        if Self::is_indexer_initializing_error(&log_error) {
                            ui.label(
                                RichText::new(i18n(
                                    "Logs are initializing while the database services are starting.",
                                ))
                                .color(theme_color().node_data_color)
                                .size(12.0),
                            );
                        } else {
                            ui.label(
                                RichText::new(log_error)
                                    .color(theme_color().warning_color)
                                    .size(12.0),
                            );
                        }
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
                        ui.label(i18n("API endpoint:"));
                        ui.label(
                            RichText::new(Self::api_url(&core.settings.self_hosted))
                                .monospace()
                                .color(theme_color().hyperlink_color),
                        );
                        if ui.small_button(i18n("Copy")).clicked() {
                            ui.ctx().copy_text(Self::api_url(&core.settings.self_hosted));
                            runtime().notify_clipboard(i18n("Copied to clipboard"));
                        }
                    });
                });
            });
    }
}
