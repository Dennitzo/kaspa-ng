use crate::imports::*;
use crate::runtime::services::kaspa::logs::Log;

pub struct RkBlocks {
    runtime: Runtime,
    header_prefix: Option<String>,
    empty_prefix: Option<String>,
}

impl RkBlocks {
    pub fn new(runtime: Runtime) -> Self {
        Self {
            runtime,
            header_prefix: None,
            empty_prefix: None,
        }
    }

    fn short_time(timestamp: &str) -> Option<String> {
        let trimmed = timestamp.trim();
        if trimmed.is_empty() {
            return None;
        }

        let mut candidate = if let Some((_, time)) = trimmed.split_once('T') {
            time
        } else if let Some(time) = trimmed.split_whitespace().find(|part| part.contains(':')) {
            time
        } else {
            trimmed
        };

        candidate = candidate.trim_end_matches('Z');
        candidate = candidate.split(['+', '-']).next().unwrap_or(candidate);

        if candidate.contains(':') {
            Some(candidate.to_string())
        } else {
            None
        }
    }

    fn time_prefix(&self, timestamp: Option<&str>) -> String {
        let time = timestamp
            .and_then(Self::short_time)
            .unwrap_or_else(Self::now_time);
        format!("{time:<12} ")
    }

    fn now_time() -> String {
        chrono::Local::now().format("%H:%M:%S%.3f").to_string()
    }

    fn status_log(&self, status: &str, line: String) -> Log {
        if status.starts_with("Accepted") {
            Log::Processed(line)
        } else if status.starts_with("Rejected") {
            Log::Error(line)
        } else {
            Log::Warning(line)
        }
    }
}

impl ModuleT for RkBlocks {
    fn name(&self) -> Option<&'static str> {
        Some(i18n("Blocks"))
    }

    fn render(
        &mut self,
        core: &mut Core,
        _ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        ui: &mut egui::Ui,
    ) {
        if !core.settings.node.stratum_bridge_enabled {
            ui.colored_label(theme_color().warning_color, i18n("RK Bridge is disabled in Settings."));
            ui.add_space(8.);
        }

        let header_lines = [
            i18n("This view lists blocks found by the RK Stratum Bridge (external mode)."),
            i18n("When a connected miner finds a block candidate, the bridge submits it to the local Rusty Kaspa node."),
            i18n("The status is updated as Accepted or Rejected after submission."),
        ];

        let blocks = self.runtime.stratum_bridge_service().blocks().clone();

        egui::ScrollArea::vertical()
            .id_salt("rk_blocks")
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let header_prefix = self.header_prefix.get_or_insert_with(|| {
                    let time = Self::now_time();
                    format!("{time:<12} ")
                });
                for line in header_lines {
                    ui.label(RichText::from(&Log::Info(format!("{header_prefix}{line}"))));
                }

                ui.label(RichText::from(&Log::Processed(format!("{header_prefix}------"))));

                if blocks.is_empty() {
                    let prefix = self.empty_prefix.get_or_insert_with(|| {
                        let time = Self::now_time();
                        format!("{time:<12} ")
                    });
                    ui.label(RichText::from(&Log::Info(format!(
                        "{prefix}{}",
                        i18n("No blocks are mined yet.")
                    ))));
                    ui.label(RichText::from(&Log::Info(format!(
                        "{prefix}{}",
                        i18n("When a miner connected to RK Stratum finds a valid block, it will appear here with its hash and status.")
                    ))));
                    return;
                }

                self.empty_prefix = None;

                for (index, block) in blocks.iter().enumerate() {
                    let prefix = self.time_prefix(block.timestamp.as_deref());
                    if index > 0 {
                        let divider = format!("{prefix}------");
                        ui.label(RichText::from(&Log::Processed(divider)));
                    }

                    let status_line = format!("{prefix}{}", block.status.as_str());
                    let status_log = self.status_log(block.status.as_str(), status_line);
                    ui.label(RichText::from(&status_log));

                    ui.horizontal(|ui| {
                        ui.label(RichText::from(&Log::Processed(format!("{prefix}Hash: "))));
                        let link_text = RichText::new(block.hash.as_str())
                            .font(FontId::monospace(theme_style().node_log_font_size))
                            .color(theme_color().logs_processed_color)
                            .underline();
                        let response = ui.add(egui::Label::new(link_text).sense(Sense::click()));
                        if response.clicked() {
                            core.settings.user_interface.explorer_last_path =
                                format!("/blocks/{}", block.hash);
                            core.store_settings();
                            core.select::<modules::Explorer>();
                        }
                    });

                    if let Some(worker) = block.worker.as_deref() {
                        if !worker.is_empty() {
                            ui.label(RichText::from(&Log::Info(format!("{prefix}Worker: {}", worker))));
                        }
                    }

                    if let Some(wallet) = block.wallet.as_deref() {
                        if !wallet.is_empty() {
                            ui.label(RichText::from(&Log::Info(format!("{prefix}Wallet: {}", wallet))));
                        }
                    }
                }
            });
    }
}
