use crate::imports::*;
#[cfg(not(target_arch = "wasm32"))]
use crate::modules::{
    clear_footer_connection_status, set_footer_connection_status, FooterConnectionHealth,
    FooterConnectionStatus,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::services::SelfHostedKasiaIndexerService;

#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
use std::sync::OnceLock;
#[cfg(not(target_arch = "wasm32"))]
use std::thread::JoinHandle;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use wry::{dpi::PhysicalPosition, dpi::PhysicalSize, Rect as WryRect, WebView, WebViewBuilder};

#[cfg(not(target_arch = "wasm32"))]
const KASIA_HOST: &str = "127.0.0.1";
#[cfg(not(target_arch = "wasm32"))]
const KASIA_PUBLIC_MAINNET_INDEXER_API: &str = "https://indexer.kasia.fyi";
#[cfg(not(target_arch = "wasm32"))]
const KASIA_PUBLIC_MAINNET_NODE_WS: &str = "wss://wrpc.kasia.fyi";
#[cfg(not(target_arch = "wasm32"))]
const KASIA_PUBLIC_TESTNET_NODE_WS: &str = "wss://dev-wrpc.kasia.fyi";
#[cfg(not(target_arch = "wasm32"))]
const WEBVIEW_SYNC_INTERVAL: Duration = Duration::from_millis(33);
#[cfg(not(target_arch = "wasm32"))]
const KASPA_TEAL: Color32 = Color32::from_rgb(111, 199, 186);
#[cfg(not(target_arch = "wasm32"))]
const KASPA_NEUTRAL_BLACK: Color32 = Color32::from_rgb(35, 31, 32);
#[cfg(not(target_arch = "wasm32"))]
const KASPA_SECONDARY_GRAY: Color32 = Color32::from_rgb(182, 182, 182);
#[cfg(not(target_arch = "wasm32"))]
const KASPA_BRIGHT_TEAL: Color32 = Color32::from_rgb(73, 234, 203);
#[cfg(not(target_arch = "wasm32"))]
const KASVAULT_PRIMARY_TEXT: Color32 = Color32::from_rgb(236, 244, 242);

#[cfg(not(target_arch = "wasm32"))]
const WEBVIEW_SHORTCUTS_JS: &str = r#"
(() => {
  if (window.__kaspaNgClipboardShortcuts) return;
  window.__kaspaNgClipboardShortcuts = true;
  const isMac = /Mac|iPhone|iPad|iPod/.test(navigator.platform);
  const isCommand = (e) => (isMac ? e.metaKey : e.ctrlKey);
  window.addEventListener("keydown", (e) => {
    if (!isCommand(e)) return;
    const key = (e.key || "").toLowerCase();
    try {
      if (key === "c") document.execCommand("copy");
      if (key === "x") document.execCommand("cut");
      if (key === "v") document.execCommand("paste");
    } catch (_) {}
  }, true);

  // Disable service workers in embedded WebView to avoid stale cache/asset mismatch
  // across app restarts and binary updates on localhost.
  try {
    if ("serviceWorker" in navigator) {
      const sw = navigator.serviceWorker;
      if (sw && sw.register) {
        sw.register = async () => ({ scope: location.origin + "/" });
      }
      sw.getRegistrations?.().then((registrations) => {
        registrations.forEach((registration) => {
          registration.unregister?.().catch(() => {});
        });
      }).catch(() => {});
    }
  } catch (_) {}
})();
"#;

#[cfg(not(target_arch = "wasm32"))]
const KASIA_EMBED_LAYOUT_FIX_JS: &str = r#"
(() => {
  if (window.__kaspaNgKasiaEmbedFix) return;
  window.__kaspaNgKasiaEmbedFix = true;

  const apply = () => {
    const html = document.documentElement;
    const body = document.body;
    const root = document.getElementById("root");
    if (!html || !body || !root) return;

    html.style.setProperty("width", "100%", "important");
    html.style.setProperty("height", "100%", "important");
    html.style.setProperty("overflow", "hidden", "important");

    body.style.setProperty("margin", "0", "important");
    body.style.setProperty("width", "100%", "important");
    body.style.setProperty("height", "100%", "important");
    body.style.setProperty("overflow", "hidden", "important");

    root.style.setProperty("width", "100%", "important");
    root.style.setProperty("height", "100%", "important");
    root.style.setProperty("max-width", "100%", "important");
    root.style.setProperty("min-width", "0", "important");
    root.style.setProperty("margin", "0", "important");

    const first = root.firstElementChild;
    if (first && first instanceof HTMLElement) {
      first.style.setProperty("width", "100%", "important");
      first.style.setProperty("height", "100%", "important");
      first.style.setProperty("max-width", "100%", "important");
      first.style.setProperty("min-width", "0", "important");
      first.style.setProperty("margin", "0", "important");
      first.style.setProperty("border-radius", "0", "important");
      first.style.setProperty("box-shadow", "none", "important");
    }
  };

  const observer = new MutationObserver(() => apply());
  observer.observe(document.documentElement, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["style", "class"],
  });

  window.addEventListener("resize", apply);
  apply();
})();
"#;

#[cfg(not(target_arch = "wasm32"))]
fn webview_bounds_from_rect(rect: egui::Rect, pixels_per_point: f32) -> WryRect {
    let scale = f64::from(pixels_per_point.max(1.0));
    #[cfg(target_os = "linux")]
    let overscan = 2.0;
    #[cfg(not(target_os = "linux"))]
    let overscan = 0.0;

    let min_x = (f64::from(rect.min.x) * scale - overscan).floor().max(0.0);
    let min_y = (f64::from(rect.min.y) * scale - overscan).floor().max(0.0);
    let max_x = (f64::from(rect.max.x) * scale + overscan).ceil();
    let max_y = (f64::from(rect.max.y) * scale + overscan).ceil();

    WryRect {
        position: PhysicalPosition::new(min_x, min_y).into(),
        size: PhysicalSize::new((max_x - min_x).max(1.0), (max_y - min_y).max(1.0)).into(),
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
fn ensure_gtk_initialized() -> std::result::Result<(), String> {
    static INIT: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    let result = INIT.get_or_init(|| gtk::init().map_err(|err| format!("{err}")));
    match result {
        Ok(()) => Ok(()),
        Err(err) => Err(err.clone()),
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
fn pump_gtk_events_bounded() {
    const MAX_ITERATIONS_PER_FRAME: usize = 8;
    for _ in 0..MAX_ITERATIONS_PER_FRAME {
        if !gtk::events_pending() {
            break;
        }
        gtk::main_iteration_do(false);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, PartialEq, Eq)]
struct KasiaRuntimeConfig {
    indexer_mainnet_url: Option<String>,
    indexer_testnet_url: Option<String>,
    default_mainnet_node_url: Option<String>,
    default_testnet_node_url: Option<String>,
}

pub struct Kasia {
    #[allow(dead_code)]
    runtime: Runtime,
    #[cfg(not(target_arch = "wasm32"))]
    server: Option<KasiaServer>,
    #[cfg(not(target_arch = "wasm32"))]
    webview: Option<WebView>,
    #[cfg(not(target_arch = "wasm32"))]
    last_bounds: Option<WryRect>,
    #[cfg(not(target_arch = "wasm32"))]
    last_zoom: Option<f64>,
    #[cfg(not(target_arch = "wasm32"))]
    last_signature: Option<(Network, bool, KasiaRuntimeConfig)>,
    #[cfg(not(target_arch = "wasm32"))]
    status: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    last_probe_at: Option<std::time::Instant>,
    #[cfg(not(target_arch = "wasm32"))]
    last_probe_ok: Option<bool>,
    #[cfg(not(target_arch = "wasm32"))]
    last_probe_status: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    last_webview_attempt: Option<std::time::Instant>,
    #[cfg(not(target_arch = "wasm32"))]
    last_webview_sync_at: Option<std::time::Instant>,
}

impl Kasia {
    pub fn new(runtime: Runtime) -> Self {
        Self {
            runtime,
            #[cfg(not(target_arch = "wasm32"))]
            server: None,
            #[cfg(not(target_arch = "wasm32"))]
            webview: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_bounds: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_zoom: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_signature: None,
            #[cfg(not(target_arch = "wasm32"))]
            status: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_probe_at: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_probe_ok: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_probe_status: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_webview_attempt: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_webview_sync_at: None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_local_server(&mut self, port: u16) {
        if self.server.is_some() {
            return;
        }
        match KasiaServer::start(port) {
            Ok(server) => self.server = Some(server),
            Err(err) => {
                self.status = Some(err);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn normalized_node_url_from_runtime(node: &crate::settings::NodeSettings) -> String {
        runtime()
            .kaspa_service()
            .rpc_url()
            .map(|url| {
                if url.starts_with("wrpcs://") {
                    url.replacen("wrpcs://", "wss://", 1)
                } else if url.starts_with("wrpc://") {
                    url.replacen("wrpc://", "ws://", 1)
                } else if url.starts_with("ws://") || url.starts_with("wss://") {
                    url
                } else {
                    format!("ws://{url}")
                }
            })
            .unwrap_or_else(|| match node.connection_config_kind {
                crate::settings::NodeConnectionConfigKind::PublicServerRandom
                | crate::settings::NodeConnectionConfigKind::PublicServerCustom => {
                    "public (resolving...)".to_string()
                }
                crate::settings::NodeConnectionConfigKind::Custom => {
                    let configured = node.wrpc_url.trim();
                    if configured.is_empty() {
                        format!(
                            "ws://127.0.0.1:{}",
                            crate::settings::node_wrpc_borsh_port_for_network(node.network)
                        )
                    } else if configured.contains("://") {
                        configured.to_string()
                    } else {
                        format!("ws://{configured}")
                    }
                }
            })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn official_node_url(core: &Core) -> String {
        if matches!(core.settings.node.network, Network::Mainnet) {
            KASIA_PUBLIC_MAINNET_NODE_WS.to_string()
        } else {
            KASIA_PUBLIC_TESTNET_NODE_WS.to_string()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn effective_node_url(core: &Core) -> String {
        if core.state().is_connected() && core.state().is_synced() {
            Self::normalized_node_url_from_runtime(&core.settings.node)
        } else {
            Self::official_node_url(core)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn runtime_config(core: &Core, use_self_hosted: bool) -> KasiaRuntimeConfig {
        let node_url = Self::effective_node_url(core);

        if !use_self_hosted {
            return KasiaRuntimeConfig {
                indexer_mainnet_url: None,
                indexer_testnet_url: None,
                default_mainnet_node_url: Some(node_url.clone()),
                default_testnet_node_url: Some(node_url),
            };
        }

        let host = if core.settings.self_hosted.api_bind == "0.0.0.0"
            || core.settings.self_hosted.api_bind == "::"
            || core.settings.self_hosted.api_bind == "[::]"
        {
            "127.0.0.1".to_string()
        } else {
            core.settings.self_hosted.api_bind.clone()
        };
        let indexer_port = SelfHostedKasiaIndexerService::health_probe_ports(
            &core.settings.self_hosted,
            &core.settings.node,
        )
        .into_iter()
        .find(|port| *port == SelfHostedKasiaIndexerService::RUNTIME_API_PORT)
        .unwrap_or_else(|| {
            core.settings
                .self_hosted
                .effective_kasia_indexer_port(core.settings.node.network)
        });
        let indexer_url = format!("http://{}:{}", host, indexer_port);

        KasiaRuntimeConfig {
            indexer_mainnet_url: Some(indexer_url.clone()),
            indexer_testnet_url: Some(indexer_url),
            default_mainnet_node_url: Some(node_url.clone()),
            default_testnet_node_url: Some(node_url),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn open_in_browser(&self, browser: crate::settings::KasvaultBrowser, url: &str) {
        if let Err(err) = crate::modules::kasvault::open_url_with_browser(browser, url) {
            self.runtime
                .toast(UserNotification::error(format!("Kasia browser open failed: {err}")));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn render_browser_card(
        &mut self,
        ui: &mut Ui,
        title: &str,
        browser: crate::settings::KasvaultBrowser,
        url: Option<String>,
    ) {
        render_kasvault_style_card(ui, |ui, compact| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(title)
                        .size(if compact { 24.0 } else { 30.0 })
                        .strong()
                        .color(KASPA_TEAL),
                );
                ui.add_space(if compact { 4.0 } else { 6.0 });
                ui.label(
                    RichText::new(i18n(
                        "Embedded WebView is disabled. Open this tab in your system browser.",
                    ))
                    .color(KASPA_SECONDARY_GRAY),
                );

                ui.add_space(if compact { 10.0 } else { 14.0 });
                let field_stroke = Stroke::new(1.0, KASPA_TEAL.linear_multiply(0.75));

                Frame::new()
                    .stroke(field_stroke)
                    .corner_radius(CornerRadius::same(12))
                    .fill(KASPA_NEUTRAL_BLACK.linear_multiply(0.92))
                    .inner_margin(if compact {
                        Margin::symmetric(10, 8)
                    } else {
                        Margin::symmetric(12, 10)
                    })
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(i18n("URL")).strong().color(KASPA_TEAL));
                            if let Some(url) = &url {
                                ui.label(
                                    RichText::new(url.as_str())
                                        .monospace()
                                        .color(KASVAULT_PRIMARY_TEXT),
                                );
                            } else {
                                ui.colored_label(theme_color().warning_color, i18n("unavailable"));
                            }
                        });
                    });

                ui.add_space(if compact { 6.0 } else { 8.0 });
                Frame::new()
                    .stroke(field_stroke)
                    .corner_radius(CornerRadius::same(12))
                    .fill(KASPA_NEUTRAL_BLACK.linear_multiply(0.92))
                    .inner_margin(if compact {
                        Margin::symmetric(10, 8)
                    } else {
                        Margin::symmetric(12, 10)
                    })
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(i18n("Browser")).strong().color(KASPA_TEAL));
                            ui.label(
                                RichText::new(browser.label())
                                    .monospace()
                                    .color(KASVAULT_PRIMARY_TEXT),
                            );
                        });
                    });

                ui.add_space(if compact { 10.0 } else { 12.0 });
                ui.horizontal_centered(|ui| {
                    let button = Button::new(
                        RichText::new(i18n("Open in Browser"))
                            .strong()
                            .color(KASPA_NEUTRAL_BLACK),
                    )
                    .fill(KASPA_TEAL)
                    .stroke(Stroke::new(1.0, KASPA_BRIGHT_TEAL))
                    .corner_radius(CornerRadius::same(10))
                    .min_size(vec2(if compact { 180.0 } else { 220.0 }, 38.0));
                    if ui.add_enabled(url.is_some(), button).clicked()
                        && let Some(url) = &url
                    {
                        self.open_in_browser(browser, url);
                    }
                });
            });
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn render_kasvault_style_card<F>(ui: &mut Ui, add_contents: F)
where
    F: FnOnce(&mut Ui, bool),
{
    let viewport_width = ui.available_width();
    let compact = viewport_width < 640.0;
    let top_padding = if compact {
        (ui.available_height() * 0.08).clamp(8.0, 48.0)
    } else {
        (ui.available_height() * 0.14).clamp(14.0, 120.0)
    };
    ui.add_space(top_padding);

    ui.vertical_centered(|ui| {
        let content_width = if compact {
            (ui.available_width() * 0.96).clamp(280.0, 560.0)
        } else {
            (ui.available_width() * 0.90).clamp(360.0, 860.0)
        };
        ui.allocate_ui_with_layout(
            vec2(content_width, 0.0),
            Layout::top_down(Align::Center),
            |ui| {
                let fill = KASPA_NEUTRAL_BLACK;
                let stroke = Stroke::new(1.0, KASPA_TEAL.linear_multiply(0.85));
                Frame::new()
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(CornerRadius::same(if compact { 14 } else { 18 }))
                    .inner_margin(if compact {
                        Margin::symmetric(14, 12)
                    } else {
                        Margin::symmetric(24, 20)
                    })
                    .show(ui, |ui| add_contents(ui, compact));
            },
        );
    });
}

impl ModuleT for Kasia {
    fn name(&self) -> Option<&'static str> {
        Some(i18n("Kasia"))
    }

    fn activate(&mut self, _core: &mut Core) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(webview) = &self.webview {
            let _ = webview.set_visible(true);
            let _ = webview.focus();
        }
    }

    fn deactivate(&mut self, _core: &mut Core) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            clear_footer_connection_status();
            if let Some(webview) = &self.webview {
                let _ = webview.set_visible(false);
            }
        }
    }

    fn hide(&mut self, _core: &mut Core) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            clear_footer_connection_status();
            if let Some(webview) = &self.webview {
                let _ = webview.set_visible(false);
            }
        }
    }

    fn show(&mut self, _core: &mut Core) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(webview) = &self.webview {
            let _ = webview.set_visible(true);
            let _ = webview.focus();
        }
    }

    fn render(
        &mut self,
        core: &mut Core,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
        ui: &mut egui::Ui,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            #[cfg(target_os = "linux")]
            {
                if let Err(err) = ensure_gtk_initialized() {
                    self.status = Some(format!("GTK init failed: {err}"));
                    ui.colored_label(theme_color().error_color, i18n("Kasia is unavailable."));
                    return;
                }
                pump_gtk_events_bounded();
            }

            if !matches!(core.settings.node.network, Network::Mainnet) {
                clear_footer_connection_status();
                if let Some(webview) = &self.webview {
                    let _ = webview.set_visible(false);
                }
                self.webview = None;
                self.server = None;
                self.last_signature = None;
                self.last_bounds = None;
                self.last_zoom = None;
                self.last_probe_ok = None;
                self.last_probe_status = None;
                self.last_probe_at = None;
                self.last_webview_attempt = None;
                self.last_webview_sync_at = None;
                ui.label(i18n("Kasia is available only on Mainnet."));
                return;
            }

            let self_hosted_requested =
                core.settings.self_hosted.enabled && core.settings.self_hosted.kasia_enabled;
            let loader_snapshot = runtime().self_hosted_loader_service().status_snapshot();
            let use_self_hosted = self_hosted_requested
                && loader_snapshot.postgres_ready
                && loader_snapshot.indexers_ready;
            let host = if core.settings.self_hosted.api_bind == "0.0.0.0"
                || core.settings.self_hosted.api_bind == "::"
                || core.settings.self_hosted.api_bind == "[::]"
            {
                "127.0.0.1".to_string()
            } else {
                core.settings.self_hosted.api_bind.clone()
            };
            let configured_port = core
                .settings
                .self_hosted
                .effective_kasia_indexer_port(core.settings.node.network);
            let probe_ports = SelfHostedKasiaIndexerService::health_probe_ports(
                &core.settings.self_hosted,
                &core.settings.node,
            );
            let api_port = configured_port;

            if use_self_hosted {
                let should_probe = self
                    .last_probe_at
                    .map(|last| last.elapsed() >= Duration::from_secs(2))
                    .unwrap_or(true);
                if should_probe {
                    let probe = kasia_indexer_health(&host, &probe_ports);
                    self.last_probe_ok = Some(probe.ready);
                    self.last_probe_status = Some(probe.status);
                    self.last_probe_at = Some(std::time::Instant::now());
                }
            } else {
                self.last_probe_ok = None;
                self.last_probe_status = None;
                self.last_probe_at = None;
            }

            let node_display = Self::effective_node_url(core);
            let api_display = if use_self_hosted {
                format!("http://{}:{}", host, api_port)
            } else {
                KASIA_PUBLIC_MAINNET_INDEXER_API.to_string()
            };
            let api_health = if use_self_hosted {
                match self.last_probe_ok {
                    Some(true) => FooterConnectionHealth::Connected,
                    Some(false) => FooterConnectionHealth::Unreachable,
                    None => FooterConnectionHealth::Unknown,
                }
            } else {
                FooterConnectionHealth::Connected
            };
            let node_health = if core.state().is_connected() {
                FooterConnectionHealth::Connected
            } else {
                FooterConnectionHealth::Reachable
            };
            set_footer_connection_status(FooterConnectionStatus {
                node: node_display.clone(),
                node_health,
                api: api_display,
                api_health,
            });

            let kasia_ui_port = core
                .settings
                .user_interface
                .effective_kasia_port(core.settings.node.network);
            self.ensure_local_server(kasia_ui_port);

            if core.settings.rendering.disable_embedded_webview {
                self.webview.take();
                self.last_bounds = None;
                self.last_zoom = None;
                self.last_signature = None;
                self.last_webview_attempt = None;
                self.last_webview_sync_at = None;

                let browser_url = self.server.as_ref().map(|server| server.url.clone());
                self.render_browser_card(
                    ui,
                    i18n("Kasia"),
                    core.settings.rendering.webview_browser,
                    browser_url,
                );
                return;
            }

            if self.server.is_none() {
                ui.colored_label(
                    theme_color().error_color,
                    self.status
                        .clone()
                        .unwrap_or_else(|| i18n("Kasia local web server is not available.").to_string()),
                );
                return;
            }

            if let Some(status) = &self.status {
                ui.colored_label(theme_color().warning_color, status);
            }
            if use_self_hosted && !matches!(self.last_probe_ok, Some(true)) {

                let status = self
                    .last_probe_status
                    .clone()
                    .unwrap_or_else(|| "unreachable".to_string());
                ui.colored_label(
                    theme_color().warning_color,
                    format!(
                        "{} http://{}:{}/metrics ({status})",
                        i18n("Waiting for Kasia Indexer API:"),
                        host,
                        api_port
                    ),
                );
            }

            let available_rect = ui.available_rect_before_wrap();
            ui.allocate_rect(available_rect, Sense::hover());
            let bounds = webview_bounds_from_rect(available_rect, ctx.pixels_per_point());
            let target_zoom = f64::from(ctx.zoom_factor().max(0.5));

            let runtime_config = Self::runtime_config(core, use_self_hosted);
            let signature = Some((core.settings.node.network, use_self_hosted, runtime_config.clone()));
            if self.webview.is_some() && self.last_signature != signature {
                self.webview.take();
                self.last_bounds = None;
                self.last_zoom = None;
                self.last_webview_sync_at = None;
            }

            if self.webview.is_none() {
                if let Some(last_attempt) = self.last_webview_attempt
                    && last_attempt.elapsed() < Duration::from_secs(2)
                {
                    return;
                }
                self.last_webview_attempt = Some(std::time::Instant::now());

                let server_url = self
                    .server
                    .as_ref()
                    .map(|server| server.url.clone())
                    .unwrap_or_else(|| format!("http://{KASIA_HOST}:{kasia_ui_port}/"));

                let config_script = kasia_runtime_config_script(
                    &runtime_config,
                );

                match WebViewBuilder::new()
                    .with_url(server_url.as_str())
                    .with_bounds(bounds)
                    .with_clipboard(true)
                    .with_accept_first_mouse(true)
                    .with_focused(true)
                    .with_initialization_script(WEBVIEW_SHORTCUTS_JS)
                    .with_initialization_script(KASIA_EMBED_LAYOUT_FIX_JS)
                    .with_initialization_script(config_script.as_str())
                    .build_as_child(frame)
                {
                    Ok(webview) => {
                        let _ = webview.set_visible(true);
                        let _ = webview.focus();
                        let _ = webview.zoom(target_zoom);
                        self.webview = Some(webview);
                        self.last_bounds = Some(bounds);
                        self.last_zoom = Some(target_zoom);
                        self.last_signature = signature;
                        self.status = None;
                        self.last_webview_attempt = None;
                        self.last_webview_sync_at = Some(std::time::Instant::now());
                    }
                    Err(err) => {
                        #[cfg(target_os = "linux")]
                        {
                            self.status = Some(format!(
                                "Kasia WebView error: {err}. Ensure webkit2gtk is installed (e.g. libwebkit2gtk-4.1-dev or webkit2gtk4.0-devel).",
                            ));
                        }
                        #[cfg(not(target_os = "linux"))]
                        {
                            self.status = Some(format!("Kasia WebView error: {err}"));
                        }
                        log_warn!("Kasia WebView error: {err}");
                    }
                }
            } else if let Some(webview) = &self.webview {
                let should_sync_webview = self
                    .last_webview_sync_at
                    .map(|last| last.elapsed() >= WEBVIEW_SYNC_INTERVAL)
                    .unwrap_or(true);
                if should_sync_webview {
                    if self.last_bounds != Some(bounds) {
                        if let Err(err) = webview.set_bounds(bounds) {
                            self.status = Some(format!("Kasia WebView resize error: {err}"));
                        } else {
                            self.last_bounds = Some(bounds);
                        }
                    }
                    if self
                        .last_zoom
                        .map(|zoom| (zoom - target_zoom).abs() > 0.001)
                        .unwrap_or(true)
                    {
                        if let Err(err) = webview.zoom(target_zoom) {
                            self.status = Some(format!("Kasia WebView zoom error: {err}"));
                        } else {
                            self.last_zoom = Some(target_zoom);
                        }
                    }
                    self.last_webview_sync_at = Some(std::time::Instant::now());
                }
                let _ = webview.set_visible(true);
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let _ = core;
            let _ = ctx;
            let _ = frame;
            ui.label(i18n("Kasia is not available in Web builds."));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct KasiaServer {
    url: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl KasiaServer {
    fn start(port: u16) -> std::result::Result<Self, String> {
        let root = find_kasia_build_root().ok_or_else(|| {
            i18n("Kasia build not found. Run `npm install` and `npm run build:production` in `Kasia`.")
                .to_string()
        })?;

        let listener = TcpListener::bind((KASIA_HOST, port)).map_err(|err| {
            format!("Kasia server bind failed on {KASIA_HOST}:{port} ({err})")
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|err| format!("Kasia server nonblocking setup failed: {err}"))?;
        let addr = listener
            .local_addr()
            .map_err(|err| format!("Kasia server address error: {err}"))?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_signal = Arc::clone(&stop);
        let url = format!("http://{}:{}/", addr.ip(), addr.port());

        let thread = std::thread::Builder::new()
            .name("kasia-server".to_string())
            .spawn(move || {
                while !stop_signal.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let root = root.clone();
                            std::thread::spawn(move || {
                                let _ = handle_kasia_request(stream, root.as_path());
                            });
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(25));
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|err| format!("Kasia server spawn failed: {err}"))?;

        Ok(Self {
            url,
            stop,
            thread: Some(thread),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for KasiaServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn find_kasia_build_root() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let is_macos_bundle = {
        #[cfg(target_os = "macos")]
        {
            std::env::current_exe()
                .ok()
                .map(|exe| exe.to_string_lossy().contains(".app/Contents/MacOS/"))
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    };

    if !is_macos_bundle && let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("Kasia").join("dist"));
        for ancestor in cwd.ancestors().skip(1).take(4) {
            candidates.push(ancestor.join("Kasia").join("dist"));
        }
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("Kasia").join("dist"));
        if is_macos_bundle {
            if let Some(contents) = dir.parent() {
                candidates.push(contents.join("Resources").join("Kasia").join("dist"));
            }
        } else {
            for ancestor in dir.ancestors().skip(1).take(4) {
                candidates.push(ancestor.join("Kasia").join("dist"));
            }
        }
    }

    candidates
        .into_iter()
        .find(|path| path.join("index.html").exists())
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_kasia_request(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let mut request = [0_u8; 4096];
    let read = stream.read(&mut request)?;
    if read == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&request[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let raw_path = path.split('?').next().unwrap_or("/");
    let clean = raw_path.trim_start_matches('/');
    if clean.contains("..") {
        return Ok(());
    }

    let mut file_path = if clean.is_empty() {
        root.join("index.html")
    } else {
        root.join(clean)
    };

    if !file_path.exists() || !file_path.is_file() {
        let looks_like_asset = clean.rsplit_once('.').is_some();
        if looks_like_asset {
            let body = b"Not Found";
            let headers =
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store, no-cache, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(headers.as_bytes())?;
            stream.write_all(body)?;
            stream.flush()?;
            return Ok(());
        }
        file_path = root.join("index.html");
    }

    let body = if file_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("index.html"))
        .unwrap_or(false)
    {
        let html = std::fs::read_to_string(&file_path).unwrap_or_default();
        strip_integrity_attributes(&html).into_bytes()
    } else {
        std::fs::read(&file_path).unwrap_or_default()
    };
    let content_type = content_type_for(file_path.as_path());
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nCache-Control: no-store, no-cache, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nConnection: close\r\n\r\n"
    );

    stream.write_all(headers.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn strip_integrity_attributes(html: &str) -> String {
    // Kasia dist is served from an embedded localhost static server.
    // If integrity hashes drift (e.g. mixed stale files), browsers block scripts entirely.
    // Stripping SRI for local trusted origin keeps the app functional.
    let mut out = String::with_capacity(html.len());
    let mut i = 0usize;
    while let Some(rel) = html[i..].find("integrity=") {
        let mut start = i + rel;
        while start > i && html.as_bytes()[start - 1].is_ascii_whitespace() {
            start -= 1;
        }
        out.push_str(&html[i..start]);

        let mut cursor = i + rel + "integrity=".len();
        let bytes = html.as_bytes();
        if cursor < html.len() && (bytes[cursor] == b'"' || bytes[cursor] == b'\'') {
            let quote = bytes[cursor];
            cursor += 1;
            while cursor < html.len() && bytes[cursor] != quote {
                cursor += 1;
            }
            if cursor < html.len() {
                cursor += 1;
            }
        } else {
            while cursor < html.len()
                && !bytes[cursor].is_ascii_whitespace()
                && bytes[cursor] != b'>'
            {
                cursor += 1;
            }
        }
        i = cursor;
    }
    out.push_str(&html[i..]);
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn kasia_runtime_config_script(config: &KasiaRuntimeConfig) -> String {
    let indexer_mainnet_url = js_string_or_undefined(&config.indexer_mainnet_url);
    let indexer_testnet_url = js_string_or_undefined(&config.indexer_testnet_url);
    let default_mainnet_node_url = js_string_or_undefined(&config.default_mainnet_node_url);
    let default_testnet_node_url = js_string_or_undefined(&config.default_testnet_node_url);

    format!(
        r#"
(() => {{
  try {{
    window.__KASPA_NG_KASIA_CONFIG = {{
      indexerMainnetUrl: {indexer_mainnet_url},
      indexerTestnetUrl: {indexer_testnet_url},
      defaultMainnetNodeUrl: {default_mainnet_node_url},
      defaultTestnetNodeUrl: {default_testnet_node_url}
    }};
  }} catch (_) {{}}
}})();
"#
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn js_string_or_undefined(value: &Option<String>) -> String {
    match value {
        Some(value) => format!("\"{}\"", escape_js_string(value)),
        None => "undefined".to_string(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn escape_js_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(not(target_arch = "wasm32"))]
struct KasiaIndexerHealth {
    ready: bool,
    status: String,
}

#[cfg(not(target_arch = "wasm32"))]
fn kasia_indexer_health(host: &str, ports: &[u16]) -> KasiaIndexerHealth {
    for port in ports {
        let target = format!("{host}:{port}");
        if let Ok(addrs) = target.to_socket_addrs() {
            for addr in addrs {
                let mut stream =
                    match TcpStream::connect_timeout(&addr, Duration::from_millis(500)) {
                        Ok(stream) => stream,
                        Err(_) => continue,
                    };
                let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

                let request = format!(
                    "GET /metrics HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n"
                );
                if stream.write_all(request.as_bytes()).is_err() {
                    return KasiaIndexerHealth {
                        ready: false,
                        status: "connected, request failed".to_string(),
                    };
                }

                let mut response = [0_u8; 256];
                let bytes_read = stream.read(&mut response).unwrap_or_default();
                if bytes_read == 0 {
                    return KasiaIndexerHealth {
                        ready: false,
                        status: "connected, empty response".to_string(),
                    };
                }

                let text = String::from_utf8_lossy(&response[..bytes_read]);
                let line = text.lines().next().unwrap_or_default();
                let code = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|value| value.parse::<u16>().ok());

                return match code {
                    Some(200..=299) => KasiaIndexerHealth {
                        ready: true,
                        status: format!("ready ({line})"),
                    },
                    Some(code) => KasiaIndexerHealth {
                        ready: false,
                        status: format!("not ready (HTTP {code})"),
                    },
                    None => KasiaIndexerHealth {
                        ready: false,
                        status: "connected, invalid HTTP response".to_string(),
                    },
                };
            }
        }
    }

    KasiaIndexerHealth {
        ready: false,
        status: "unreachable".to_string(),
    }
}
