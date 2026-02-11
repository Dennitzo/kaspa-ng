use crate::imports::*;
use egui_extras::{Column, TableBuilder};

pub struct RkBlocks {
    runtime: Runtime,
}

impl RkBlocks {
    pub fn new(runtime: Runtime) -> Self {
        Self { runtime }
    }

    fn status_color(&self, status: &str) -> Color32 {
        if status.starts_with("Accepted") {
            theme_color().ack_color
        } else if status.starts_with("Rejected") {
            theme_color().error_color
        } else {
            theme_color().warning_color
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

        ui.label(i18n(
            "This view lists blocks found by the RK Stratum Bridge (external mode). \
When a connected miner finds a block candidate, the bridge submits it to the local Rusty Kaspa node \
and updates the status as Accepted or Rejected.",
        ));
        ui.add_space(12.);

        let blocks = self.runtime.stratum_bridge_service().blocks();

        if blocks.is_empty() {
            ui.colored_label(theme_color().info_color, i18n("No blocks are mined yet."));
            ui.label(i18n(
                "When a miner connected to RK Stratum finds a valid block, it will appear here with its hash and status.",
            ));
            return;
        }

        let total = blocks.len();
        let accepted = blocks
            .iter()
            .filter(|block| block.status.starts_with("Accepted"))
            .count();
        let rejected = blocks
            .iter()
            .filter(|block| block.status.starts_with("Rejected"))
            .count();
        let found = total.saturating_sub(accepted + rejected);

        ui.horizontal(|ui| {
            ui.strong(i18n("Summary"));
            ui.label(format!("{} {}", i18n("Total:"), total));
            ui.colored_label(theme_color().ack_color, format!("{} {}", i18n("Accepted:"), accepted));
            ui.colored_label(
                theme_color().error_color,
                format!("{} {}", i18n("Rejected:"), rejected),
            );
            if found > 0 {
                ui.colored_label(
                    theme_color().warning_color,
                    format!("{} {}", i18n("Found:"), found),
                );
            }
        });
        ui.add_space(8.);

        egui::ScrollArea::vertical()
            .id_salt("rk_blocks")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                use egui_phosphor::light::CLIPBOARD_TEXT;

                let row_height = ui.text_style_height(&TextStyle::Body) + 6.0;
                let header_height = ui.text_style_height(&TextStyle::Body) + 8.0;

                TableBuilder::new(ui)
                    .striped(true)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::auto())
                    .column(Column::auto())
                    .column(Column::remainder().at_least(240.0))
                    .column(Column::auto())
                    .column(Column::auto())
                    .column(Column::auto())
                    .header(header_height, |mut header| {
                        header.col(|ui| {
                            ui.strong(i18n("Time"));
                        });
                        header.col(|ui| {
                            ui.strong(i18n("Status"));
                        });
                        header.col(|ui| {
                            ui.strong(i18n("Hash"));
                        });
                        header.col(|ui| {
                            ui.strong(i18n("Worker"));
                        });
                        header.col(|ui| {
                            ui.strong(i18n("Wallet"));
                        });
                        header.col(|ui| {
                            ui.strong(i18n(""));
                        });
                    })
                    .body(|mut body| {
                        for block in blocks.iter().rev() {
                            body.row(row_height, |mut row| {
                                row.col(|ui| {
                                    ui.label(block.timestamp.as_deref().unwrap_or("-"));
                                });
                                row.col(|ui| {
                                    let status_color = self.status_color(block.status.as_str());
                                    let status_text =
                                        RichText::new(block.status.as_str()).color(status_color).strong();
                                    let background = status_color.gamma_multiply(0.18);
                                    Frame::NONE
                                        .fill(background)
                                        .corner_radius(CornerRadius::same(4))
                                        .inner_margin(Margin::symmetric(6, 2))
                                        .show(ui, |ui| {
                                            ui.label(status_text);
                                        });
                                });
                                row.col(|ui| {
                                    let hash_display = format_partial_string(block.hash.as_str(), Some(12));
                                    let hash_label = Label::new(
                                        RichText::new(hash_display).monospace().color(ui.visuals().strong_text_color()),
                                    )
                                    .sense(Sense::click());
                                    if ui.add(hash_label).on_hover_text(block.hash.as_str()).clicked() {
                                        ui.ctx().copy_text(block.hash.clone());
                                        runtime().notify_clipboard(i18n("Block hash copied"));
                                    }
                                });
                                row.col(|ui| {
                                    let worker = block.worker.as_deref().unwrap_or("-");
                                    ui.label(RichText::new(worker).monospace());
                                });
                                row.col(|ui| {
                                    let wallet = block.wallet.as_deref().unwrap_or("-");
                                    let wallet_display = if wallet == "-" {
                                        "-".to_string()
                                    } else {
                                        format_partial_string(wallet, Some(10))
                                    };
                                    let wallet_label = Label::new(RichText::new(wallet_display).monospace())
                                        .sense(Sense::hover());
                                    if wallet != "-" {
                                        ui.add(wallet_label).on_hover_text(wallet);
                                    } else {
                                        ui.add(wallet_label);
                                    }
                                });
                                row.col(|ui| {
                                    let copy_button = Button::new(RichText::new(CLIPBOARD_TEXT).size(14.))
                                        .frame(true)
                                        .min_size(vec2(24.0, 18.0));
                                    if ui
                                        .add(copy_button)
                                        .on_hover_text(i18n("Copy hash"))
                                        .clicked()
                                    {
                                        ui.ctx().copy_text(block.hash.clone());
                                        runtime().notify_clipboard(i18n("Block hash copied"));
                                    }
                                });
                            });
                        }
                    });
            });
    }
}
