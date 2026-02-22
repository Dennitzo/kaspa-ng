use crate::imports::*;

pub struct Settings {
    #[allow(dead_code)]
    runtime: Runtime,
    settings : crate::settings::Settings,
    wrpc_borsh_network_interface : NetworkInterfaceEditor,
    wrpc_json_network_interface : NetworkInterfaceEditor,
    grpc_network_interface : NetworkInterfaceEditor,
    reset_settings : bool,
    reset_database : bool,
}

impl Settings {
    pub fn new(runtime: Runtime) -> Self {
        Self { 
            runtime,
            settings : crate::settings::Settings::default(),
            wrpc_borsh_network_interface : NetworkInterfaceEditor::default(),
            wrpc_json_network_interface : NetworkInterfaceEditor::default(),
            grpc_network_interface : NetworkInterfaceEditor::default(),
            reset_settings : false,
            reset_database : false,
        }
    }

    pub fn load(&mut self, settings : crate::settings::Settings) {
        self.settings = settings;

        self.wrpc_borsh_network_interface = NetworkInterfaceEditor::from(&self.settings.node.wrpc_borsh_network_interface);
        self.wrpc_json_network_interface = NetworkInterfaceEditor::from(&self.settings.node.wrpc_json_network_interface);
        self.grpc_network_interface = NetworkInterfaceEditor::from(&self.settings.node.grpc_network_interface);
    }

    pub fn change_current_network(&mut self, network : Network) {
        self.settings.node.network = network;
        if crate::settings::should_auto_sync_self_hosted_explorer_profiles(
            &self.settings.explorer.self_hosted,
        ) {
            self.settings.explorer.self_hosted =
                crate::settings::self_hosted_explorer_profiles_from_settings(
                    &self.settings.self_hosted,
                );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn connect_host_for_bind(bind: &str) -> &str {
        match bind.trim() {
            "0.0.0.0" | "::" | "[::]" | "" => "127.0.0.1",
            other => other,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn port_accepts_connections(host: &str, port: u16) -> bool {
        use std::net::{TcpStream, ToSocketAddrs};
        use std::time::Duration;

        let Ok(addresses) = (host, port).to_socket_addrs() else {
            return false;
        };

        addresses.into_iter().any(|addr| {
            TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn wait_for_self_hosted_ready(settings: &SelfHostedSettings, network: Network) -> bool {
        use std::thread::sleep;
        use std::time::{Duration, Instant};

        let host = Self::connect_host_for_bind(&settings.api_bind);
        let deadline = Instant::now() + Duration::from_secs(15);

        while Instant::now() < deadline {
            let rest_ready =
                Self::port_accepts_connections(host, settings.effective_explorer_rest_port(network));
            let socket_ready = Self::port_accepts_connections(
                host,
                settings.effective_explorer_socket_port(network),
            );
            let indexer_ready =
                Self::port_accepts_connections(host, settings.effective_api_port(network));
            let postgres_ready = Self::port_accepts_connections(
                &settings.db_host,
                settings.effective_db_port(network),
            );

            if rest_ready && socket_ready && indexer_ready && postgres_ready {
                return true;
            }

            sleep(Duration::from_millis(350));
        }

        false
    }

    pub fn render_remote_settings(_core: &mut Core, ui: &mut Ui, settings : &mut NodeSettings) -> Option<&'static str> {

        let mut node_settings_error = None;

        CollapsingHeader::new(i18n("Remote p2p Node Configuration"))
        .default_open(true)
        .show(ui, |ui| {


            ui.horizontal_wrapped(|ui|{
                ui.label(i18n("Remote Connection:"));
                NodeConnectionConfigKind::iter().for_each(|kind| {
                    ui.radio_value(&mut settings.connection_config_kind, *kind, kind.to_string());
                });
            });

            match settings.connection_config_kind {
                NodeConnectionConfigKind::Custom => {

                    CollapsingHeader::new(i18n("wRPC Connection Settings"))
                        .default_open(true)
                        .show(ui, |ui| {


                            ui.horizontal(|ui|{
                                ui.label(i18n("wRPC Encoding:"));
                                WrpcEncoding::iter().for_each(|encoding| {
                                    ui.radio_value(&mut settings.wrpc_encoding, *encoding, encoding.to_string());
                                });
                            });


                            ui.horizontal(|ui|{
                                ui.label(i18n("wRPC URL:"));
                                ui.add(TextEdit::singleline(&mut settings.wrpc_url));
                                
                            });

                            if let Err(err) = KaspaRpcClient::parse_url(settings.wrpc_url.clone(), settings.wrpc_encoding, settings.network.into()) {
                                ui.label(
                                    RichText::new(err.to_string())
                                        .color(theme_color().warning_color),
                                );
                                node_settings_error = Some(i18n("Invalid wRPC URL"));
                            }
                        });
                    // cfg_if! {
                    //     if #[cfg(not(target_arch = "wasm32"))] {
                    //         ui.horizontal_wrapped(|ui|{
                    //             ui.label(i18n("Recommended arguments for the remote node: "));
                    //             ui.label(RichText::new("kaspad --utxoindex --rpclisten-borsh=0.0.0.0").code().font(FontId::monospace(14.0)).color(theme_color().strong_color));
                    //         });
                    //         ui.horizontal_wrapped(|ui|{
                    //             ui.label(i18n("If you are running locally, use: "));
                    //             ui.label(RichText::new("--rpclisten-borsh=127.0.0.1.").code().font(FontId::monospace(14.0)).color(theme_color().strong_color));
                    //         });
                    //     }
                    // }

                },
                NodeConnectionConfigKind::PublicServerCustom => {
                },
                NodeConnectionConfigKind::PublicServerRandom => {
                    ui.label(i18n("A random node will be selected on startup"));
                },
            }

        });

        node_settings_error
    }
}

impl ModuleT for Settings {

    fn init(&mut self, wallet : &mut Core) {
        self.load(wallet.settings.clone());
    }

    fn style(&self) -> ModuleStyle {
        ModuleStyle::Default
    }

    fn render(
        &mut self,
        core: &mut Core,
        _ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        ui: &mut egui::Ui,
    ) {
        ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                self.render_settings(core,ui);
            });
    }

    fn deactivate(&mut self, _core: &mut Core) {
        #[cfg(not(target_arch = "wasm32"))]
        _core.storage.clear_settings();
    }

}

impl Settings {

    fn render_node_settings(
        &mut self,
        core: &mut Core,
        ui: &mut egui::Ui,
    ) {
        #[allow(unused_variables)]
        let half_width = ui.ctx().screen_rect().width() * 0.5;

        let mut node_settings_error = None;

        CollapsingHeader::new(i18n("Kaspa p2p Network & Node Connection"))
            .default_open(true)
            .show(ui, |ui| {


                CollapsingHeader::new(i18n("Kaspa Network"))
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui|{
                            Network::iter().for_each(|network| {
                                ui.radio_value(&mut self.settings.node.network, *network, network.name());
                            });
                        });
                    });


                CollapsingHeader::new(i18n("Kaspa Node"))
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui|{
                            KaspadNodeKind::iter().for_each(|node_kind| {
                                #[cfg(not(target_arch = "wasm32"))] {
                                    if !core.settings.developer.experimental_features_enabled() && matches!(*node_kind,KaspadNodeKind::IntegratedInProc|KaspadNodeKind::ExternalAsDaemon) {
                                        return;
                                    }
                                }
                                ui.radio_value(&mut self.settings.node.node_kind, *node_kind, node_kind.to_string()).on_hover_text_at_pointer(node_kind.describe());
                            });
                        });

                        match self.settings.node.node_kind {
                            KaspadNodeKind::Remote => {

                            },

                            #[cfg(not(target_arch = "wasm32"))]
                            KaspadNodeKind::IntegratedInProc => {
                                ui.horizontal_wrapped(|ui|{
                                    ui.set_max_width(half_width);
                                    ui.label(i18n("Please note that the integrated mode is experimental and does not currently show the sync progress information."));
                                });
                            },

                            #[cfg(not(target_arch = "wasm32"))]
                            KaspadNodeKind::ExternalAsDaemon => {

                                ui.horizontal(|ui|{
                                    ui.label(i18n("Rusty Kaspa Daemon Path:"));
                                    ui.add(TextEdit::singleline(&mut self.settings.node.kaspad_daemon_binary));
                                });

                                let path = std::path::PathBuf::from(&self.settings.node.kaspad_daemon_binary);
                                if path.exists() && !path.is_file() {
                                    ui.label(
                                        RichText::new(format!("Rusty Kaspa Daemon not found at '{path}'", path = self.settings.node.kaspad_daemon_binary))
                                            .color(theme_color().error_color),
                                    );
                                    node_settings_error = Some("Rusty Kaspa Daemon not found");
                                }
                            },
                            _ => { }
                        }

                        #[cfg(not(target_arch = "wasm32"))]
                        if self.settings.node.node_kind.is_config_capable() {

                            CollapsingHeader::new(i18n("Cache Memory Size"))
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui|{
                                        NodeMemoryScale::iter().for_each(|kind| {
                                            ui.radio_value(&mut self.settings.node.memory_scale, *kind, kind.to_string());
                                        });
                                    });
                                    ui.label(self.settings.node.memory_scale.describe());
                                });
                        }

                        #[cfg(not(target_arch = "wasm32"))]
                        if self.settings.node.node_kind.is_config_capable() {
                            CollapsingHeader::new(i18n("Data Storage"))
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.checkbox(&mut self.settings.node.kaspad_daemon_storage_folder_enable, i18n("Custom data storage folder"));
                                    if self.settings.node.kaspad_daemon_args.contains("--appdir") && self.settings.node.kaspad_daemon_storage_folder_enable {
                                        ui.colored_label(theme_color().warning_color, i18n("Your daemon arguments contain '--appdir' directive, which overrides the data storage folder setting."));
                                        ui.colored_label(theme_color().warning_color, i18n("Please remove the --appdir directive to continue."));
                                    } else if self.settings.node.kaspad_daemon_storage_folder_enable {
                                        ui.horizontal(|ui|{
                                            ui.label(i18n("Data Storage Folder:"));
                                            ui.add(TextEdit::singleline(&mut self.settings.node.kaspad_daemon_storage_folder));
                                        });

                                        let appdir = self.settings.node.kaspad_daemon_storage_folder.trim();
                                        if appdir.is_empty() {
                                            ui.colored_label(theme_color().error_color, i18n("Data storage folder must not be empty"));
                                        } else if !Path::new(appdir).exists() {
                                            ui.colored_label(theme_color().error_color, i18n("Data storage folder not found at"));
                                            ui.label(format!("\"{}\"",self.settings.node.kaspad_daemon_storage_folder.trim()));

                                            ui.add_space(4.);
                                            if ui.medium_button(i18n("Create Data Folder")).clicked() {
                                                if let Err(err) = std::fs::create_dir_all(appdir) {
                                                    runtime().error(format!("Unable to create data storage folder `{appdir}`: {err}"));
                                                }
                                            }
                                            ui.add_space(4.);

                                            node_settings_error = Some(i18n("Data storage folder not found"));
                                        }
                                    }
                                });
                        }

                        #[cfg(not(target_arch = "wasm32"))]
                        if core.settings.developer.custom_daemon_args_enabled() && self.settings.node.node_kind.is_config_capable() {
                            use kaspad_lib::args::Args;
                            use clap::error::ErrorKind as ClapErrorKind;
                            use crate::runtime::services::kaspa::Config;

                            ui.horizontal(|ui| {
                                ui.add_space(2.);
                                ui.checkbox(&mut self.settings.node.kaspad_daemon_args_enable, i18n("Activate custom daemon arguments"));
                            });

                            if self.settings.node.kaspad_daemon_args_enable {
                                ui.indent("kaspad_daemon_args", |ui| {
                                    ui.vertical(|ui| {
                                        ui.label(i18n("Resulting daemon arguments:"));
                                        ui.add_space(4.);

                                        let config = Config::from(self.settings.node.clone());
                                        let config = Vec::<String>::from(config).join(" ");
                                        ui.label(RichText::new(config).code().font(FontId::monospace(14.0)).color(theme_color().strong_color));
                                        ui.add_space(4.);


                                        ui.label(i18n("Custom arguments:"));
                                        let width = ui.available_width() * 0.4;
                                        let height = 48.0;
                                        ui.add_sized(vec2(width,height),TextEdit::multiline(&mut self.settings.node.kaspad_daemon_args).code_editor().font(FontId::monospace(14.0)));
                                        ui.add_space(4.);
                                    });

                                    let args = format!("kaspad {}",self.settings.node.kaspad_daemon_args.trim());
                                    let args = args.trim().split(' ').collect::<Vec<&str>>();
                                    match Args::parse(args.iter()) {
                                        Ok(_) => { },
                                        Err(err) => {

                                            if matches!(err.kind(), ClapErrorKind::DisplayHelp | ClapErrorKind::DisplayVersion) {
                                                ui.label(
                                                    RichText::new("--help and --version are not allowed")
                                                        .color(theme_color().warning_color),
                                                );
                                            } else {
                                                let help = err.to_string();
                                                let lines = help.split('\n').collect::<Vec<&str>>();
                                                let text = if let Some(idx) = lines.iter().position(|line| line.starts_with("For more info") || line.starts_with("Usage:")) {
                                                    lines[0..idx].join("\n")
                                                } else {
                                                    lines.join("\n")
                                                };

                                                ui.label(
                                                    RichText::new(text.trim())
                                                        .color(theme_color().warning_color),
                                                );
                                            }
                                            ui.add_space(4.);
                                            node_settings_error = Some(i18n("Invalid daemon arguments"));
                                        }
                                    }
                                });
                            }
                        }

                    });

                if !self.grpc_network_interface.is_valid() {
                    node_settings_error = Some(i18n("Invalid gRPC network interface configuration"));
                } else {
                    self.settings.node.grpc_network_interface = self.grpc_network_interface.as_ref().try_into().unwrap(); //NetworkInterfaceConfig::try_from(&self.grpc_network_interface).unwrap();
                }

                if self.settings.node.node_kind == KaspadNodeKind::Remote {
                    node_settings_error = Self::render_remote_settings(core, ui, &mut self.settings.node);
                }

                #[cfg(not(target_arch = "wasm32"))]
                if self.settings.node.node_kind.is_config_capable() {

                    CollapsingHeader::new(i18n("Local p2p Node Configuration"))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.vertical(|ui|{
                                CollapsingHeader::new(i18n("Client RPC"))
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        ui.vertical(|ui|{

                                            ui.checkbox(&mut self.settings.node.enable_wrpc_borsh, i18n("Public wRPC (Borsh)"));

                                            // ui.checkbox(&mut self.settings.node.enable_wrpc_json, i18n("Enable wRPC JSON"));
                                            // if self.settings.node.enable_wrpc_json {
                                            //     CollapsingHeader::new(i18n("wRPC JSON Network Interface & Port"))
                                            //         .default_open(true)
                                            //         .show(ui, |ui| {
                                            //             self.wrpc_json_network_interface.ui(ui);
                                            //         });
                                            // }

                                            ui.checkbox(&mut self.settings.node.enable_grpc, i18n("Enable gRPC"));
                                            if self.settings.node.enable_grpc {
                                                CollapsingHeader::new(i18n("gRPC Network Interface & Port"))
                                                    .default_open(true)
                                                    .show(ui, |ui| {
                                                        self.grpc_network_interface.ui(ui);
                                                    });
                                            }
                                        });

                                });
                            // });
                            
                                CollapsingHeader::new(i18n("p2p RPC"))
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        ui.vertical(|ui|{
                                            ui.checkbox(&mut self.settings.node.enable_upnp, i18n("Enable UPnP"));
                                        });
                                    });
                                });
                        });
                } // is_config_capable

            }); // Kaspa p2p Network & Node Connection

            if let Some(error) = node_settings_error {
                ui.add_space(4.);
                ui.label(
                    RichText::new(error)
                        .color(theme_color().error_color),
                );
                ui.add_space(4.);
                ui.label(i18n("Unable to change node settings until the problem is resolved"));

                ui.add_space(8.);

                if let Some(response) = ui.confirm_medium_cancel(Align::Max) {
                    if matches!(response, Confirm::Nack) {
                        self.settings.node = core.settings.node.clone();
                        self.grpc_network_interface = NetworkInterfaceEditor::from(&self.settings.node.grpc_network_interface);
                    }
                }

                ui.separator();

            } else if node_settings_error.is_none() {
                if let Some(restart) = self.settings.node.compare(&core.settings.node) {

                    ui.add_space(16.);
                    if let Some(response) = ui.confirm_medium_apply_cancel(Align::Max) {
                        match response {
                            Confirm::Ack => {

                                core.settings = self.settings.clone();
                                if !matches!(core.settings.node.network, Network::Mainnet) {
                                    core.settings.node.stratum_bridge_enabled = false;
                                    self.settings.node.stratum_bridge_enabled = false;
                                    core.settings.self_hosted.k_enabled = false;
                                    self.settings.self_hosted.k_enabled = false;
                                }
                                core.settings.store_sync().unwrap();

                                cfg_if! {
                                    if #[cfg(not(target_arch = "wasm32"))] {
                                        let storage_root = core.settings.node.kaspad_daemon_storage_folder_enable.then_some(core.settings.node.kaspad_daemon_storage_folder.as_str());
                                        core.storage.track_storage_root(storage_root);
                                    }
                                }

                                self.runtime
                                    .stratum_bridge_service()
                                    .update_settings(&core.settings.node);
                                if !matches!(core.settings.node.network, Network::Mainnet) {
                                    self.runtime
                                        .stratum_bridge_service()
                                        .enable(false, &core.settings.node);
                                }
                                self.runtime.cpu_miner_service().update_settings(
                                    core.settings.node.network,
                                    &core.settings.node.cpu_miner,
                                );
                                self.runtime.rothschild_service().update_settings(
                                    core.settings.node.network,
                                    &core.settings.node.rothschild,
                                );
                                #[cfg(not(target_arch = "wasm32"))]
                                self.runtime
                                    .self_hosted_db_service()
                                    .update_node_settings(core.settings.node.clone());
                                #[cfg(not(target_arch = "wasm32"))]
                                self.runtime
                                    .self_hosted_postgres_service()
                                    .update_node_settings(core.settings.node.clone());
                                #[cfg(not(target_arch = "wasm32"))]
                                self.runtime
                                    .self_hosted_indexer_service()
                                    .update_node_settings(core.settings.node.clone());
                                #[cfg(not(target_arch = "wasm32"))]
                                self.runtime
                                    .self_hosted_explorer_service()
                                    .update_node_settings(core.settings.node.clone());
                                #[cfg(not(target_arch = "wasm32"))]
                                self.runtime
                                    .self_hosted_indexer_service()
                                    .update_node_settings(core.settings.node.clone());
                                #[cfg(not(target_arch = "wasm32"))]
                                self.runtime
                                    .self_hosted_k_indexer_service()
                                    .update_node_settings(core.settings.node.clone());
                                #[cfg(not(target_arch = "wasm32"))]
                                self.runtime.self_hosted_k_indexer_service().enable(
                                    core.settings.self_hosted.enabled
                                        && core.settings.self_hosted.k_enabled
                                        && matches!(core.settings.node.network, Network::Mainnet),
                                );

                                if restart {
                                    self.runtime.kaspa_service().update_services(&self.settings.node, None);
                                }
                            },
                            Confirm::Nack => {
                                self.settings = core.settings.clone();
                                self.grpc_network_interface = NetworkInterfaceEditor::from(&self.settings.node.grpc_network_interface);
                            }
                        }
                    }
                    ui.separator();
                }
            }
    }




    fn render_ui_settings(
        &mut self,
        core: &mut Core,
        ui: &mut egui::Ui,
    ) {


        CollapsingHeader::new(i18n("User Interface"))
            .default_open(false)
            .show(ui, |ui| {

                CollapsingHeader::new(i18n("Theme Color"))
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let theme_color = theme_color();
                                let current_theme_color_name = theme_color.name();
                                ui.menu_button(
                                    format!("{} ⏷", current_theme_color_name),
                                    |ui| {
                                        theme_colors().keys().for_each(|name| {
                                            if name.as_str() != current_theme_color_name
                                                && ui.button(name).clicked()
                                            {
                                                apply_theme_color_by_name(
                                                    ui.ctx(),
                                                    name,
                                                );
                                                core
                                                    .settings
                                                    .user_interface
                                                    .theme_color = name.to_string();
                                                core.store_settings();
                                                ui.close_menu();
                                            }
                                        });
                                    },
                                );
                            });
                        });

                        ui.add_space(1.);
                    });

                    CollapsingHeader::new(i18n("Theme Style"))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let theme_style = theme_style();
                                let current_theme_style_name = theme_style.name();
                                ui.menu_button(
                                    format!("{} ⏷", current_theme_style_name),
                                    |ui| {
                                        theme_styles().keys().for_each(|name| {
                                            if name.as_str() != current_theme_style_name
                                                && ui.button(name).clicked()
                                            {
                                                apply_theme_style_by_name(ui.ctx(), name);
                                                core
                                                    .settings
                                                    .user_interface
                                                    .theme_style = name.to_string();
                                                core.store_settings();
                                                ui.close_menu();
                                            }
                                        });
                                    },
                                );
                            });
                            ui.add_space(1.);
                        });

                        if workflow_core::runtime::is_native() {
                            CollapsingHeader::new(i18n("Zoom"))
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let zoom_factor = ui.ctx().zoom_factor();
                                        if ui
                                            .add_sized(
                                                Vec2::splat(24.),
                                                Button::new(RichText::new("-").size(18.)),
                                            )
                                            .clicked()
                                        {
                                            ui.ctx().set_zoom_factor(zoom_factor - 0.1);
                                        }
                                        ui.label(format!("{:.0}%", zoom_factor * 100.0));
                                        if ui
                                            .add_sized(
                                                Vec2::splat(24.),
                                                Button::new(RichText::new("+").size(18.)),
                                            )
                                            .clicked()
                                        {
                                            ui.ctx().set_zoom_factor(zoom_factor + 0.1);
                                        }
                                    });

                                    ui.add_space(1.);
                                });
                        }

                        if workflow_core::runtime::is_native() {

                            CollapsingHeader::new(i18n("Options"))
                                .default_open(true)
                                .show(ui, |ui| {

                                    ui.checkbox(&mut self.settings.user_interface.disable_frame, i18n("Disable Window Frame"));

                                    let restart_required =
                                        self.settings.user_interface.disable_frame
                                            != core.settings.user_interface.disable_frame;

                                    if restart_required {
                                        ui.vertical(|ui| {
                                            ui.add_space(4.);
                                            ui.label(RichText::new(i18n("Application must be restarted for this setting to take effect.")).color(theme_color().warning_color));
                                            ui.label(RichText::new(i18n("Please select 'Apply' and restart the application.")).color(theme_color().warning_color));
                                            ui.add_space(4.);
                                        });
                                    }

                                    ui.add_space(1.);
                                });

                            if self.settings.user_interface.disable_frame != core.settings.user_interface.disable_frame
                            {
                                ui.add_space(16.);
                                if let Some(response) = ui.confirm_medium_apply_cancel(Align::Max) {
                                    match response {
                                        Confirm::Ack => {
                                            core.settings.user_interface.disable_frame = self.settings.user_interface.disable_frame;
                                            core.settings.store_sync().unwrap();
                                        },
                                        Confirm::Nack => {
                                            self.settings.user_interface.disable_frame = core.settings.user_interface.disable_frame;
                                        }
                                    }
                                }
                                ui.separator();
                            }
                        }
            });

    }

    fn render_settings(
        &mut self,
        core: &mut Core,
        ui: &mut egui::Ui,
    ) {

        self.render_node_settings(core,ui);

        self.render_ui_settings(core,ui);

        CollapsingHeader::new(i18n("Services"))
            .default_open(true)
            .show(ui, |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                CollapsingHeader::new(i18n("Self Hosted"))
                    .default_open(true)
                    .show(ui, |ui| {
                        use egui_phosphor::light::CLIPBOARD_TEXT;

                        let mut changed = false;
                        let mut settings = self.settings.self_hosted.clone();
                        let is_mainnet = matches!(core.settings.node.network, Network::Mainnet);
                        let self_hosted_enabled = settings.enabled;

                        if !is_mainnet && settings.k_enabled {
                            settings.k_enabled = false;
                            changed = true;
                        }
                        if !self_hosted_enabled && settings.k_enabled {
                            settings.k_enabled = false;
                            changed = true;
                        }

                        if ui
                            .checkbox(
                                &mut settings.enabled,
                                i18n("Enable self-hosted database services"),
                            )
                            .changed()
                        {
                            changed = true;
                        }

                        if ui
                            .add_enabled(
                                is_mainnet && self_hosted_enabled,
                                Checkbox::new(&mut settings.k_enabled, i18n("Enable K-Social services")),
                            )
                            .changed()
                        {
                            changed = true;
                        }

                        if !is_mainnet {
                            ui.colored_label(
                                theme_color().warning_color,
                                i18n("K-Social services are available only on Mainnet."),
                            );
                        } else if !self_hosted_enabled {
                            ui.colored_label(
                                theme_color().warning_color,
                                i18n("Enable self-hosted database services first to use K-Social."),
                            );
                        }

                        ui.add_space(6.);
                        ui.separator();
                        ui.add_space(6.);

                        Grid::new("self_hosted_settings_grid")
                            .num_columns(2)
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(i18n("API Bind Address"));
                                changed |= ui
                                    .add(TextEdit::singleline(&mut settings.api_bind).desired_width(200.0))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("API Port"));
                                changed |= ui
                                    .add(DragValue::new(&mut settings.api_port).range(1..=65535))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("K API Port"));
                                changed |= ui
                                    .add(DragValue::new(&mut settings.k_web_port).range(1..=65535))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Explorer REST Port"));
                                changed |= ui
                                    .add(DragValue::new(&mut settings.explorer_rest_port).range(1..=65535))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Explorer Socket Port"));
                                changed |= ui
                                    .add(
                                        DragValue::new(&mut settings.explorer_socket_port)
                                            .range(1..=65535),
                                    )
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Database Host"));
                                changed |= ui
                                    .add(TextEdit::singleline(&mut settings.db_host).desired_width(200.0))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Database Port"));
                                changed |= ui
                                    .add(DragValue::new(&mut settings.db_port).range(1..=65535))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Database User"));
                                changed |= ui
                                    .add(TextEdit::singleline(&mut settings.db_user).desired_width(200.0))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Database Password"));
                                ui.horizontal(|ui| {
                                    changed |= ui
                                        .add(
                                            TextEdit::singleline(&mut settings.db_password)
                                                .desired_width(220.0),
                                        )
                                        .changed();
                                    if ui.small_button(i18n("Regenerate")).clicked() {
                                        settings.db_password = crate::settings::generate_db_password();
                                        changed = true;
                                    }
                                    if ui
                                        .small_button(RichText::new(format!(" {CLIPBOARD_TEXT} ")))
                                        .clicked()
                                    {
                                        ui.ctx().copy_text(settings.db_password.clone());
                                        runtime().notify_clipboard(i18n("Copied to clipboard"));
                                    }
                                });
                                ui.end_row();

                                let mut effective_db_name =
                                    crate::settings::self_hosted_db_name_for_network(
                                        &settings.db_name,
                                        core.settings.node.network,
                                    );
                                ui.label(i18n("Database Name"));
                                ui.add_enabled(
                                    false,
                                    TextEdit::singleline(&mut effective_db_name)
                                        .desired_width(200.0),
                                );
                                ui.end_row();

                                ui.label(i18n("Database Base Name"));
                                changed |= ui
                                    .add(TextEdit::singleline(&mut settings.db_name).desired_width(200.0))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Indexer RPC URL"));
                                changed |= ui
                                    .add(
                                        TextEdit::singleline(&mut settings.indexer_rpc_url)
                                            .desired_width(260.0),
                                    )
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Indexer Listen"));
                                changed |= ui
                                    .add(
                                        TextEdit::singleline(&mut settings.indexer_listen)
                                            .desired_width(200.0),
                                    )
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Indexer Extra Args"));
                                changed |= ui
                                    .add(
                                        TextEdit::singleline(&mut settings.indexer_extra_args)
                                            .desired_width(360.0),
                                    )
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Upgrade DB"));
                                changed |= ui
                                    .checkbox(&mut settings.indexer_upgrade_db, i18n("Enable --upgrade-db"))
                                    .changed();
                                ui.end_row();
                            });

                        if changed {
                            if !is_mainnet || !settings.enabled {
                                settings.k_enabled = false;
                            }
                            if settings.db_password.trim().is_empty() || settings.db_password == "kaspa" {
                                settings.db_password = crate::settings::generate_db_password();
                            }
                            // These toggles are intentionally hidden in UI and should stay enabled.
                            settings.postgres_enabled = true;
                            settings.indexer_enabled = true;

                            if crate::settings::should_auto_sync_self_hosted_explorer_profiles(
                                &self.settings.explorer.self_hosted,
                            ) {
                                let synced_profiles =
                                    crate::settings::self_hosted_explorer_profiles_from_settings(
                                        &settings,
                                    );
                                self.settings.explorer.self_hosted = synced_profiles.clone();
                                core.settings.explorer.self_hosted = synced_profiles;
                            }

                            let previous_enabled = core.settings.self_hosted.enabled;
                            self.settings.self_hosted = settings.clone();
                            core.settings.self_hosted = settings.clone();

                            self.runtime
                                .self_hosted_db_service()
                                .update_settings(core.settings.self_hosted.clone());
                            self.runtime
                                .self_hosted_db_service()
                                .update_node_settings(core.settings.node.clone());
                            self.runtime
                                .self_hosted_explorer_service()
                                .update_settings(core.settings.self_hosted.clone());
                            self.runtime
                                .self_hosted_explorer_service()
                                .update_node_settings(core.settings.node.clone());
                            self.runtime
                                .self_hosted_postgres_service()
                                .update_settings(core.settings.self_hosted.clone());
                            self.runtime
                                .self_hosted_postgres_service()
                                .update_node_settings(core.settings.node.clone());
                            self.runtime
                                .self_hosted_indexer_service()
                                .update_settings(core.settings.self_hosted.clone());
                            self.runtime
                                .self_hosted_indexer_service()
                                .update_node_settings(core.settings.node.clone());
                            self.runtime
                                .self_hosted_k_indexer_service()
                                .update_settings(core.settings.self_hosted.clone());
                            self.runtime
                                .self_hosted_k_indexer_service()
                                .update_node_settings(core.settings.node.clone());

                            if previous_enabled != settings.enabled {
                                self.runtime.self_hosted_postgres_service().enable(
                                    settings.enabled && core.settings.self_hosted.postgres_enabled,
                                );
                                self.runtime.self_hosted_indexer_service().enable(
                                    settings.enabled && core.settings.self_hosted.indexer_enabled,
                                );
                                self.runtime
                                    .self_hosted_db_service()
                                    .enable(settings.enabled);
                                self.runtime
                                    .self_hosted_explorer_service()
                                    .enable(settings.enabled);
                                self.runtime
                                    .self_hosted_k_indexer_service()
                                    .enable(
                                        settings.enabled
                                            && core.settings.self_hosted.k_enabled
                                            && is_mainnet,
                                    );
                            } else {
                                self.runtime
                                    .self_hosted_k_indexer_service()
                                    .enable(
                                        settings.enabled
                                            && core.settings.self_hosted.k_enabled
                                            && is_mainnet,
                                    );
                            }

                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                let switched_to_enabled = !previous_enabled && settings.enabled;
                                let switched_to_disabled = previous_enabled && !settings.enabled;

                                if switched_to_enabled {
                                    if Self::wait_for_self_hosted_ready(
                                        &settings,
                                        core.settings.node.network,
                                    ) {
                                        if !matches!(
                                            core.settings.explorer.source,
                                            ExplorerDataSource::SelfHosted
                                        ) {
                                            core.settings.explorer.source =
                                                ExplorerDataSource::SelfHosted;
                                            self.settings.explorer.source =
                                                ExplorerDataSource::SelfHosted;
                                            self.runtime.toast(
                                                UserNotification::success(i18n(
                                                    "Self-hosted services are ready. Explorer switched to Self-hosted.",
                                                )),
                                            );
                                        }
                                    } else {
                                        core.settings.explorer.source = ExplorerDataSource::Official;
                                        self.settings.explorer.source = ExplorerDataSource::Official;
                                        self.runtime.toast(
                                            UserNotification::warning(i18n(
                                                "Self-hosted services are not fully reachable yet (REST/Socket/Indexer/Postgres). Explorer stays on Official.",
                                            )),
                                        );
                                    }
                                } else if switched_to_disabled
                                    && matches!(
                                        core.settings.explorer.source,
                                        ExplorerDataSource::SelfHosted
                                    )
                                {
                                    core.settings.explorer.source = ExplorerDataSource::Official;
                                    self.settings.explorer.source = ExplorerDataSource::Official;
                                    self.runtime.toast(UserNotification::info(i18n(
                                        "Self-hosted services disabled. Explorer switched to Official.",
                                    )));
                                }
                            }

                            core.store_settings();
                        }

                        ui.add_space(6.);
                        ui.separator();
                        ui.add_space(6.);
                    });

                #[cfg(not(target_arch = "wasm32"))]
                CollapsingHeader::new(i18n("RK Bridge"))
                    .default_open(true)
                    .show(ui, |ui| {
                        let is_mainnet = matches!(core.settings.node.network, Network::Mainnet);
                        let can_enable = core.settings.node.node_kind.is_local() && is_mainnet;
                        let mut enabled = self.settings.node.stratum_bridge_enabled;

                        let response = ui.add_enabled(
                            can_enable,
                            Checkbox::new(&mut enabled, i18n("Enable RK Bridge (Stratum)")),
                        );

                        if !core.settings.node.node_kind.is_local() {
                            ui.colored_label(
                                theme_color().warning_color,
                                i18n("RK Bridge requires a local node."),
                            );
                        } else if !is_mainnet {
                            ui.colored_label(
                                theme_color().warning_color,
                                i18n("RK Bridge is available only on Mainnet."),
                            );
                        } else if !core.settings.node.enable_grpc {
                            ui.colored_label(
                                theme_color().warning_color,
                                i18n("Enable gRPC in Node settings to use RK Bridge."),
                            );
                        }

                        if response.changed() {
                            self.settings.node.stratum_bridge_enabled = enabled;
                            core.settings.node.stratum_bridge_enabled = enabled;
                            self.runtime
                                .stratum_bridge_service()
                                .enable(enabled, &core.settings.node);
                            core.store_settings();
                        }

                        ui.add_space(6.);
                        ui.separator();
                        ui.add_space(6.);

                        let mut changed = false;
                        let bridge = &mut self.settings.node.stratum_bridge;

                        let mut extranonce_size = bridge.extranonce_size as u32;

                        ui.add_enabled_ui(can_enable, |ui| {
                            Grid::new("rk_bridge_settings_grid")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label(i18n("Stratum Port"));
                                    changed |= ui
                                        .add(
                                            TextEdit::singleline(&mut bridge.stratum_port)
                                                .desired_width(140.0),
                                        )
                                        .changed();
                                    ui.end_row();

                                    let local_ip = local_ip_for_stratum();
                                    let stratum_port = stratum_port_for_display(&bridge.stratum_port);
                                    let mut stratum_url =
                                        format!("stratum+tcp://{local_ip}:{stratum_port}");
                                    ui.label(i18n("Miner connection"));
                                    ui.add(
                                        TextEdit::singleline(&mut stratum_url)
                                            .desired_width(260.0)
                                            .interactive(false),
                                    );
                                    ui.end_row();

                                    ui.label(i18n("Min Share Difficulty"));
                                    changed |= ui
                                        .add(
                                            DragValue::new(&mut bridge.min_share_diff)
                                                .speed(1)
                                                .range(1..=u32::MAX),
                                        )
                                        .changed();
                                    ui.end_row();

                                    ui.label(i18n("Var Diff"));
                                    changed |=
                                        ui.checkbox(&mut bridge.var_diff, i18n("Enabled")).changed();
                                    ui.end_row();

                                    ui.label(i18n("Shares Per Min"));
                                    changed |= ui
                                        .add(
                                            DragValue::new(&mut bridge.shares_per_min)
                                                .speed(1)
                                                .range(1..=10_000),
                                        )
                                        .changed();
                                    ui.end_row();

                                    ui.label(i18n("Var Diff Stats"));
                                    changed |= ui
                                        .checkbox(&mut bridge.var_diff_stats, i18n("Enabled"))
                                        .changed();
                                    ui.end_row();

                                    ui.label(i18n("Pow2 Clamp"));
                                    changed |=
                                        ui.checkbox(&mut bridge.pow2_clamp, i18n("Enabled")).changed();
                                    ui.end_row();

                                    ui.label(i18n("Block Wait (ms)"));
                                    changed |= ui
                                        .add(
                                            DragValue::new(&mut bridge.block_wait_time_ms)
                                                .speed(50)
                                                .range(1..=120_000),
                                        )
                                        .changed();
                                    ui.end_row();

                                    ui.label(i18n("Print Stats"));
                                    changed |= ui
                                        .checkbox(&mut bridge.print_stats, i18n("Enabled"))
                                        .changed();
                                    ui.end_row();

                                    ui.label(i18n("Log To File"));
                                    changed |= ui
                                        .checkbox(&mut bridge.log_to_file, i18n("Enabled"))
                                        .changed();
                                    ui.end_row();

                                    ui.label(i18n("Extranonce Size"));
                                    let response = ui
                                        .add(DragValue::new(&mut extranonce_size).speed(1).range(0..=3));
                                    if response.changed() {
                                        changed = true;
                                        bridge.extranonce_size = extranonce_size as u8;
                                    }
                                    ui.end_row();

                                    ui.label(i18n("Coinbase Tag Suffix"));
                                    changed |= ui
                                        .add(
                                            TextEdit::singleline(&mut bridge.coinbase_tag_suffix)
                                                .desired_width(160.0)
                                                .hint_text(i18n("optional")),
                                        )
                                        .changed();
                                    ui.end_row();
                                });
                        });

                        if changed {
                            core.settings.node.stratum_bridge = bridge.clone();
                            self.runtime
                                .stratum_bridge_service()
                                .update_settings(&core.settings.node);
                            core.store_settings();
                        }
                    });

                #[cfg(not(target_arch = "wasm32"))]
                CollapsingHeader::new(i18n("CPU Miner"))
                    .default_open(true)
                    .show(ui, |ui| {
                        let is_testnet = matches!(
                            core.settings.node.network,
                            Network::Testnet10 | Network::Testnet12
                        );
                        let can_enable = core.settings.node.node_kind.is_local() && is_testnet;
                        let mut enabled = self.settings.node.cpu_miner_enabled;

                        let response = ui.add_enabled(
                            can_enable,
                            Checkbox::new(&mut enabled, i18n("Enable CPU Miner")),
                        );

                        if !core.settings.node.node_kind.is_local() {
                            ui.colored_label(
                                theme_color().warning_color,
                                i18n("CPU Miner requires a local node."),
                            );
                        } else if !is_testnet {
                            ui.colored_label(
                                theme_color().warning_color,
                                i18n("CPU Miner is available only on Testnet 10 and Testnet 12."),
                            );
                        } else if !core.settings.node.enable_grpc {
                            ui.colored_label(
                                theme_color().warning_color,
                                i18n("Enable gRPC in Node settings to use CPU Miner."),
                            );
                        }

                        if response.changed() {
                            self.settings.node.cpu_miner_enabled = enabled;
                            core.settings.node.cpu_miner_enabled = enabled;
                            self.runtime.cpu_miner_service().enable(
                                enabled,
                                core.settings.node.network,
                                &core.settings.node.cpu_miner,
                            );
                            core.store_settings();
                        }

                        ui.add_space(6.);
                        ui.separator();
                        ui.add_space(6.);

                        let mut changed = false;
                        let miner = &mut self.settings.node.cpu_miner;

                        Grid::new("cpu_miner_settings_grid")
                            .num_columns(2)
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(i18n("Mining Address"));
                                changed |= ui
                                    .add(TextEdit::singleline(&mut miner.mining_address).desired_width(260.0))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Kaspad Address"));
                                changed |= ui
                                    .add(TextEdit::singleline(&mut miner.kaspad_address).desired_width(160.0))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Kaspad Port"));
                                changed |= ui
                                    .add(DragValue::new(&mut miner.kaspad_port).speed(1).range(1..=u16::MAX))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Threads"));
                                changed |= ui
                                    .add(DragValue::new(&mut miner.threads).speed(1).range(1..=u16::MAX))
                                    .changed();
                                ui.end_row();

                            });

                        if changed {
                            core.settings.node.cpu_miner = miner.clone();
                            self.runtime.cpu_miner_service().update_settings(
                                core.settings.node.network,
                                &core.settings.node.cpu_miner,
                            );
                            core.store_settings();
                        }
                    });

                #[cfg(not(target_arch = "wasm32"))]
                CollapsingHeader::new(i18n("Rothschild"))
                    .default_open(true)
                    .show(ui, |ui| {
                        let is_testnet = matches!(
                            core.settings.node.network,
                            Network::Testnet10 | Network::Testnet12
                        );
                        let can_enable = core.settings.node.node_kind.is_local() && is_testnet;
                        let mut enabled = self.settings.node.rothschild_enabled;

                        let response = ui.add_enabled(
                            can_enable,
                            Checkbox::new(&mut enabled, i18n("Enable Rothschild")),
                        );

                        if !core.settings.node.node_kind.is_local() {
                            ui.colored_label(
                                theme_color().warning_color,
                                i18n("Rothschild requires a local node."),
                            );
                        } else if !is_testnet {
                            ui.colored_label(
                                theme_color().warning_color,
                                i18n("Rothschild is available only on Testnet 10 and Testnet 12."),
                            );
                        } else if !core.settings.node.enable_grpc {
                            ui.colored_label(
                                theme_color().warning_color,
                                i18n("Enable gRPC in Node settings to use Rothschild."),
                            );
                        }

                        if response.changed() {
                            self.settings.node.rothschild_enabled = enabled;
                            core.settings.node.rothschild_enabled = enabled;
                            if enabled {
                                let mut settings_changed = false;

                                if core.settings.node.rothschild.private_key.trim().is_empty() {
                                    let (private_key, address) =
                                        generate_rothschild_credentials(core.settings.node.network);
                                    core.settings.node.rothschild.private_key = private_key;
                                    core.settings.node.rothschild.address = address;
                                    settings_changed = true;
                                    if let Ok(mnemonic) = rothschild_mnemonic_from_private_key(
                                        &core.settings.node.rothschild.private_key,
                                    ) {
                                        core.settings.node.rothschild.mnemonic = mnemonic;
                                    }
                                } else if core.settings.node.rothschild.address.trim().is_empty() {
                                    if let Ok(address) = rothschild_address_from_private_key(
                                        core.settings.node.network,
                                        &core.settings.node.rothschild.private_key,
                                    ) {
                                        core.settings.node.rothschild.address = address;
                                        settings_changed = true;
                                    }
                                }
                                if core.settings.node.rothschild.mnemonic.trim().is_empty()
                                    && core.settings.node.rothschild.private_key.trim().is_not_empty()
                                {
                                    if let Ok(mnemonic) = rothschild_mnemonic_from_private_key(
                                        &core.settings.node.rothschild.private_key,
                                    ) {
                                        core.settings.node.rothschild.mnemonic = mnemonic;
                                        settings_changed = true;
                                    }
                                }

                                if core.settings.node.cpu_miner.mining_address.trim().is_empty()
                                    && core.settings.node.rothschild.address.trim().is_not_empty()
                                {
                                    core.settings.node.cpu_miner.mining_address =
                                        core.settings.node.rothschild.address.clone();
                                    settings_changed = true;
                                }

                                if settings_changed {
                                    self.settings.node.rothschild = core.settings.node.rothschild.clone();
                                    self.settings.node.cpu_miner = core.settings.node.cpu_miner.clone();
                                    self.runtime.rothschild_service().update_settings(
                                        core.settings.node.network,
                                        &core.settings.node.rothschild,
                                    );
                                    self.runtime.cpu_miner_service().update_settings(
                                        core.settings.node.network,
                                        &core.settings.node.cpu_miner,
                                    );
                                }
                            }
                            self.runtime.rothschild_service().enable(
                                enabled,
                                core.settings.node.network,
                                &core.settings.node.rothschild,
                            );
                            core.store_settings();
                        }

                        ui.add_space(6.);
                        ui.separator();
                        ui.add_space(6.);

                        let mut changed = false;
                        let mut private_key_changed = false;
                        let rothschild = &mut self.settings.node.rothschild;
                        use egui_phosphor::light::CLIPBOARD_TEXT;

                        Grid::new("rothschild_settings_grid")
                            .num_columns(2)
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(i18n("Wallet Mnemonic (Import in Wallet)"));
                                ui.horizontal(|ui| {
                                    let response = ui.add(
                                        TextEdit::singleline(&mut rothschild.mnemonic)
                                            .desired_width(260.0)
                                            .interactive(false),
                                    );
                                    if ui
                                        .small_button(RichText::new(format!(" {CLIPBOARD_TEXT} ")))
                                        .clicked()
                                    {
                                        ui.ctx().copy_text(rothschild.mnemonic.clone());
                                        runtime().notify_clipboard(i18n("Copied to clipboard"));
                                    }
                                    response.on_hover_text(i18n("Read-only"));
                                });
                                ui.end_row();

                                ui.label(i18n("Private Key"));
                                ui.horizontal(|ui| {
                                    let private_key_response = ui.add(
                                        TextEdit::singleline(&mut rothschild.private_key)
                                            .desired_width(260.0)
                                            .hint_text(i18n("leave empty to generate")),
                                    );
                                    if private_key_response.changed() {
                                        changed = true;
                                        private_key_changed = true;
                                    }
                                    if ui
                                        .small_button(RichText::new(format!(" {CLIPBOARD_TEXT} ")))
                                        .clicked()
                                    {
                                        ui.ctx().copy_text(rothschild.private_key.clone());
                                        runtime().notify_clipboard(i18n("Copied to clipboard"));
                                    }
                                });
                                ui.end_row();

                                ui.label(i18n("Address"));
                                ui.horizontal(|ui| {
                                    changed |= ui
                                        .add(TextEdit::singleline(&mut rothschild.address).desired_width(260.0))
                                        .changed();
                                    if ui
                                        .small_button(RichText::new(format!(" {CLIPBOARD_TEXT} ")))
                                        .clicked()
                                    {
                                        ui.ctx().copy_text(rothschild.address.clone());
                                        runtime().notify_clipboard(i18n("Copied to clipboard"));
                                    }
                                });
                                ui.end_row();

                                ui.label(i18n("RPC Server"));
                                changed |= ui
                                    .add(TextEdit::singleline(&mut rothschild.rpc_server).desired_width(160.0))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Transactions Per Second"));
                                changed |= ui
                                    .add(DragValue::new(&mut rothschild.tps).speed(1).range(1..=50))
                                    .changed();
                                ui.end_row();

                                ui.label(i18n("Threads"));
                                changed |= ui
                                    .add(DragValue::new(&mut rothschild.threads).speed(1).range(0..=u8::MAX))
                                    .changed();
                                ui.end_row();
                            });

                        if changed {
                            if private_key_changed && rothschild.private_key.trim().is_not_empty() {
                                if let Ok(mnemonic) =
                                    rothschild_mnemonic_from_private_key(&rothschild.private_key)
                                {
                                    rothschild.mnemonic = mnemonic;
                                }

                                if let Ok(address) = rothschild_address_from_private_key(
                                    core.settings.node.network,
                                    &rothschild.private_key,
                                ) {
                                    rothschild.address = address;
                                }
                            }

                            if rothschild.mnemonic.trim().is_empty()
                                && rothschild.private_key.trim().is_not_empty()
                            {
                                if let Ok(mnemonic) =
                                    rothschild_mnemonic_from_private_key(&rothschild.private_key)
                                {
                                    rothschild.mnemonic = mnemonic;
                                }
                            }

                            if rothschild.address.trim().is_empty()
                                && rothschild.private_key.trim().is_not_empty()
                            {
                                if let Ok(address) = rothschild_address_from_private_key(
                                    core.settings.node.network,
                                    &rothschild.private_key,
                                ) {
                                    rothschild.address = address;
                                }
                            }

                            if rothschild.private_key.trim().is_empty() {
                                let (private_key, address) =
                                    generate_rothschild_credentials(core.settings.node.network);
                                rothschild.private_key = private_key;
                                rothschild.address = address;
                                if let Ok(mnemonic) =
                                    rothschild_mnemonic_from_private_key(&rothschild.private_key)
                                {
                                    rothschild.mnemonic = mnemonic;
                                }
                            }

                            core.settings.node.rothschild = rothschild.clone();
                            self.runtime.rothschild_service().update_settings(
                                core.settings.node.network,
                                &core.settings.node.rothschild,
                            );

                            if core.settings.node.cpu_miner.mining_address.trim().is_empty()
                                && rothschild.address.trim().is_not_empty()
                            {
                                core.settings.node.cpu_miner.mining_address = rothschild.address.clone();
                                self.settings.node.cpu_miner = core.settings.node.cpu_miner.clone();
                                self.runtime.cpu_miner_service().update_settings(
                                    core.settings.node.network,
                                    &core.settings.node.cpu_miner,
                                );
                            }
                            core.store_settings();
                        }
                    });

                CollapsingHeader::new(i18n("Market Monitor"))
                    .default_open(true)
                    .show(ui, |ui| {
                        if ui.checkbox(&mut self.settings.market_monitor, i18n("Enable Market Monitor")).changed() {
                            core.settings.market_monitor = self.settings.market_monitor;
                            self.runtime.market_monitor_service().enable(core.settings.market_monitor);
                            core.store_settings();
                        }
                    });

                #[cfg(not(target_arch = "wasm32"))]
                CollapsingHeader::new(i18n("Check for Updates"))
                    .default_open(true)
                    .show(ui, |ui| {
                        if ui.checkbox(&mut self.settings.update_monitor, i18n("Check for Software Updates on GitHub")).changed() {
                            core.settings.update_monitor = self.settings.update_monitor;
                            self.runtime.update_monitor_service().enable(core.settings.update_monitor);
                            core.store_settings();
                        }
                    });    
            });

        CollapsingHeader::new(i18n("Network Fee Estimator"))
            .default_open(false)
            .show(ui, |ui| {
                ui.vertical(|ui|{
                    EstimatorMode::iter().for_each(|kind| {
                        ui.radio_value(&mut self.settings.estimator.mode, *kind, i18n(kind.describe()));
                    });
                    
                    if self.settings.estimator.mode != core.settings.estimator.mode {
                        core.settings.estimator.mode = self.settings.estimator.mode;
                        core.store_settings();
                    }
                });
            });
            
        #[cfg(not(target_arch = "wasm32"))]
        core.storage.clone().render_settings(core, ui);

        CollapsingHeader::new(i18n("Advanced"))
            .default_open(false)
            .show(ui, |ui| {

                ui.horizontal(|ui| {
                    ui.add_space(2.);
                    ui.vertical(|ui|{
                        ui.checkbox(&mut self.settings.developer.enable, i18n("Developer Mode"));
                        if !self.settings.developer.enable {
                            ui.label(i18n("Developer mode enables advanced and experimental features"));
                        }
                    });
                });

                if self.settings.developer.enable {
                    ui.indent("developer_mode_settings", |ui | {

                        #[cfg(not(target_arch = "wasm32"))]
                        ui.checkbox(
                            &mut self.settings.developer.enable_experimental_features, 
                            i18n("Enable experimental features")
                        ).on_hover_text_at_pointer(
                            i18n("Enables features currently in development")
                        );
                        
                        #[cfg(not(target_arch = "wasm32"))]
                        ui.checkbox(
                            &mut self.settings.developer.enable_custom_daemon_args, 
                            i18n("Enable custom daemon arguments")
                        ).on_hover_text_at_pointer(
                            i18n("Allow custom arguments for the Rusty Kaspa daemon")
                        );
                        
                        ui.checkbox(
                            &mut self.settings.developer.disable_password_restrictions, 
                            i18n("Disable password safety rules")
                        ).on_hover_text_at_pointer(
                            i18n("Removes security restrictions, allows for single-letter passwords")
                        );
                        
                        ui.checkbox(
                            &mut self.settings.developer.market_monitor_on_testnet, 
                            i18n("Show balances in alternate currencies for testnet coins")
                        ).on_hover_text_at_pointer(
                            i18n("Shows balances in alternate currencies (BTC, USD) when using testnet coins as if you are on mainnet")
                        );

                        #[cfg(not(target_arch = "wasm32"))]
                        ui.checkbox(
                            &mut self.settings.developer.enable_screen_capture, 
                            i18n("Enable screen capture")
                        ).on_hover_text_at_pointer(
                            i18n("Allows you to take screenshots from within the application")
                        );
                    });
                }

                if self.settings.developer != core.settings.developer {
                    ui.add_space(16.);
                    if let Some(response) = ui.confirm_medium_apply_cancel(Align::Max) {
                        match response {
                            Confirm::Ack => {
                                core.settings.developer = self.settings.developer.clone();
                                core.settings.store_sync().unwrap();
                            },
                            Confirm::Nack => {
                                self.settings.developer = core.settings.developer.clone();
                            }
                        }
                    }
                    ui.separator();
                }

                if !self.reset_settings {
                    ui.vertical(|ui|{
                        if self.settings.developer == core.settings.developer {
                            ui.set_max_width(340.);
                            ui.separator();
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        if !self.reset_database {
                            if ui
                                .medium_button(i18n("Reset Database"))
                                .on_hover_text(i18n(
                                    "Drops and recreates self-hosted databases for mainnet, testnet-10 and testnet-12.",
                                ))
                                .clicked()
                            {
                                self.reset_database = true;
                            }
                        }
                        if ui.medium_button(i18n("Reset Settings")).clicked() {
                            self.reset_settings = true;
                        }
                    });
                } else {
                    ui.add_space(16.);
                    ui.label(RichText::new(i18n("Are you sure you want to reset all settings?")).color(theme_color().warning_color));
                    ui.add_space(16.);
                    if let Some(response) = ui.confirm_medium_apply_cancel(Align::Min) {
                        match response {
                            Confirm::Ack => {
                                let settings = crate::settings::Settings {
                                    initialized : true,
                                    ..Default::default()
                                };
                                self.settings = settings.clone();
                                settings.store_sync().unwrap();
                                #[cfg(target_arch = "wasm32")]
                                workflow_dom::utils::window().location().reload().ok();
                            },
                            Confirm::Nack => {
                                self.reset_settings = false;
                            }
                        }
                    }
                    ui.separator();
                }

                #[cfg(not(target_arch = "wasm32"))]
                if self.reset_database {
                    ui.add_space(16.);
                    ui.label(
                        RichText::new(i18n(
                            "Are you sure you want to reset all self-hosted databases (mainnet, testnet-10, testnet-12)?",
                        ))
                        .color(theme_color().warning_color),
                    );
                    ui.add_space(16.);
                    if let Some(response) = ui.confirm_medium_apply_cancel(Align::Min) {
                        match response {
                            Confirm::Ack => {
                                self.runtime.self_hosted_postgres_service().reset_databases();
                                self.reset_database = false;
                            }
                            Confirm::Nack => {
                                self.reset_database = false;
                            }
                        }
                    }
                    ui.separator();
                }

            });
    }

}

#[cfg(not(target_arch = "wasm32"))]
fn local_ip_for_stratum() -> String {
    static CACHED: OnceLock<String> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            local_ip_address::local_ip()
                .map(|ip| match ip {
                    std::net::IpAddr::V4(v4) => v4.to_string(),
                    std::net::IpAddr::V6(v6) => format!("[{v6}]"),
                })
                .unwrap_or_else(|_| "127.0.0.1".to_string())
        })
        .clone()
}

#[cfg(target_arch = "wasm32")]
fn local_ip_for_stratum() -> String {
    "127.0.0.1".to_string()
}

fn stratum_port_for_display(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "5555".to_string();
    }

    let candidate = trimmed.rsplit(':').next().unwrap_or(trimmed).trim();
    if !candidate.is_empty() && candidate.chars().all(|c| c.is_ascii_digit()) {
        return candidate.to_string();
    }

    if trimmed.starts_with(':') {
        let rest = trimmed.trim_start_matches(':').trim();
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return rest.to_string();
        }
    }

    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        return trimmed.to_string();
    }

    "5555".to_string()
}
