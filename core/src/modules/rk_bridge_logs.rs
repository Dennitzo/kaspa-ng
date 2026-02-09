use crate::imports::*;

pub struct RkBridgeLogs {
    runtime: Runtime,
}

impl RkBridgeLogs {
    pub fn new(runtime: Runtime) -> Self {
        Self { runtime }
    }
}

impl ModuleT for RkBridgeLogs {
    fn name(&self) -> Option<&'static str> {
        Some("RK Bridge")
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

        if !core.settings.node.stratum_bridge_enabled {
            ui.colored_label(theme_color().warning_color, i18n("RK Bridge is disabled in Settings."));
            ui.add_space(8.);
        }

        #[cfg(not(target_arch = "wasm32"))]
        egui::ScrollArea::vertical()
            .id_salt("rk_bridge_logs")
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for log in self.runtime.stratum_bridge_service().logs().iter() {
                    ui.label(RichText::from(log));
                }
            });

        let copy_to_clipboard = Button::new(RichText::new(format!(" {CLIPBOARD_TEXT} ")).size(20.));

        let button_rect = Rect::from_min_size(
            pos2(available_width - 48.0, core.device().top_offset() + 32.0),
            vec2(38.0, 20.0),
        );

        if ui
            .put(button_rect, copy_to_clipboard)
            .on_hover_text_at_pointer(i18n("Copy logs to clipboard"))
            .clicked()
        {
            let logs = self
                .runtime
                .stratum_bridge_service()
                .logs()
                .iter()
                .map(|log| log.to_string())
                .collect::<Vec<String>>()
                .join("\n");
            ui.ctx().copy_text(logs);
            runtime().notify_clipboard(i18n("Copied to clipboard"));
        }
    }
}
