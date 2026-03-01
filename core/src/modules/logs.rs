use crate::imports::*;

fn is_grpc_info_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("GRPC,")
        || trimmed.contains(" GRPC,")
        || trimmed.contains("\tGRPC,")
}

pub struct Logs {
    #[allow(dead_code)]
    runtime: Runtime,
}

impl Logs {
    pub fn new(runtime: Runtime) -> Self {
        Self {
            runtime,
        }
    }
}

impl ModuleT for Logs {
    fn name(&self) -> Option<&'static str> {
        Some(i18n("Rusty Kaspa"))
    }

    fn render(
        &mut self,
        core: &mut Core,
        _ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        ui: &mut egui::Ui,
    ) {
        use egui_phosphor::light::CLIPBOARD_TEXT;

        let available_width = ui.available_width();
        let remove_grpc_info = core.settings.node.remove_grpc_info_in_rusty_kaspa_log;

        #[cfg(not(target_arch = "wasm32"))]
        egui::ScrollArea::vertical()
            .id_salt("node_logs")
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let logs = self.runtime.kaspa_service().logs();
                let mut shown = 0usize;
                for log in logs.iter() {
                    if remove_grpc_info && is_grpc_info_line(&log.to_string()) {
                        continue;
                    }
                    ui.label(RichText::from(log));
                    shown += 1;
                }

                if shown == 0 && !logs.is_empty() && remove_grpc_info {
                    ui.colored_label(
                        theme_color().warning_color,
                        i18n(
                            "All current Rusty Kaspa logs are hidden by the gRPC log filter. Disable 'remove grpc info in rusty kaspa log' in Settings to view them.",
                        ),
                    );
                } else if shown == 0 {
                    ui.colored_label(
                        theme_color().warning_color,
                        i18n("No Rusty Kaspa logs available yet."),
                    );
                }
            });

        let copy_to_clipboard = Button::new(RichText::new(format!(" {CLIPBOARD_TEXT} ")).size(20.));

        let button_rect = Rect::from_min_size(
            pos2(available_width - 48.0, core.device().top_offset() + 32.0),
            vec2(38.0, 20.0),
        );

        if ui.put(button_rect, copy_to_clipboard)
            .on_hover_text_at_pointer(i18n("Copy logs to clipboard"))
            .clicked() {
                let logs = self
                    .runtime
                    .kaspa_service()
                    .logs()
                    .iter()
                    .filter_map(|log| {
                        let line = log.to_string();
                        if remove_grpc_info && is_grpc_info_line(&line) {
                            None
                        } else {
                            Some(line)
                        }
                    })
                    .collect::<Vec<String>>()
                    .join("\n");
                //ui.output_mut(|o| o.copied_text = logs);
                ui.ctx().copy_text(logs);
                runtime().notify_clipboard(i18n("Copied to clipboard"));
            }
    }
}
