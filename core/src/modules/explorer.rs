use crate::imports::*;
#[cfg(not(target_arch = "wasm32"))]
use crate::modules::{
    clear_footer_connection_status, set_footer_connection_status, FooterConnectionHealth,
    FooterConnectionStatus,
};
use crate::settings::self_hosted_explorer_profiles_from_settings;

#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpListener, TcpStream};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::thread::JoinHandle;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_os = "linux")]
use std::sync::OnceLock;

#[cfg(not(target_arch = "wasm32"))]
use wry::{dpi::PhysicalPosition, dpi::PhysicalSize, Rect as WryRect, WebView, WebViewBuilder};

#[cfg(not(target_arch = "wasm32"))]
const EXPLORER_HOST: &str = "127.0.0.1";
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_EXPLORER_PORT: u16 = 19118;
#[cfg(not(target_arch = "wasm32"))]
const WEBVIEW_COPY_JS: &str = r#"
(() => {
  try {
    if (document.activeElement && (document.activeElement.tagName === "INPUT" || document.activeElement.tagName === "TEXTAREA")) {
      document.execCommand("copy");
      return;
    }
    document.execCommand("copy");
  } catch (_) {}
})();
"#;
#[cfg(not(target_arch = "wasm32"))]
const WEBVIEW_CUT_JS: &str = r#"
(() => {
  try {
    document.execCommand("cut");
  } catch (_) {}
})();
"#;
#[cfg(not(target_arch = "wasm32"))]
const WEBVIEW_PASTE_JS: &str = r#"
(() => {
  const active = document.activeElement;
  const insertText = (text) => {
    if (!active) return;
    if (active.isContentEditable) {
      document.execCommand("insertText", false, text);
      return;
    }
    if ("value" in active) {
      const start = active.selectionStart ?? active.value.length;
      const end = active.selectionEnd ?? active.value.length;
      if (typeof active.setRangeText === "function") {
        active.setRangeText(text, start, end, "end");
      } else {
        active.value = active.value.slice(0, start) + text + active.value.slice(end);
      }
      active.dispatchEvent(new Event("input", { bubbles: true }));
    }
  };

  if (navigator.clipboard && navigator.clipboard.readText) {
    navigator.clipboard.readText()
      .then(insertText)
      .catch(() => {
        try { document.execCommand("paste"); } catch (_) {}
      });
  } else {
    try { document.execCommand("paste"); } catch (_) {}
  }
})();
"#;
#[cfg(not(target_arch = "wasm32"))]
const WEBVIEW_SHORTCUTS_JS: &str = r#"
(() => {
  if (window.__kaspaNgClipboardShortcuts) return;
  window.__kaspaNgClipboardShortcuts = true;

  const isMac = /Mac|iPhone|iPad|iPod/.test(navigator.platform);
  const isCommand = (e) => (isMac ? e.metaKey : e.ctrlKey);

  const insertText = (text) => {
    const active = document.activeElement;
    if (!active) return;
    if (active.isContentEditable) {
      document.execCommand("insertText", false, text);
      return;
    }
    if ("value" in active) {
      const start = active.selectionStart ?? active.value.length;
      const end = active.selectionEnd ?? active.value.length;
      if (typeof active.setRangeText === "function") {
        active.setRangeText(text, start, end, "end");
      } else {
        active.value = active.value.slice(0, start) + text + active.value.slice(end);
      }
      active.dispatchEvent(new Event("input", { bubbles: true }));
    }
  };

  window.addEventListener("keydown", async (e) => {
    if (!isCommand(e)) return;
    const key = (e.key || "").toLowerCase();
    if (key === "c") {
      document.execCommand("copy");
      return;
    }
    if (key === "x") {
      document.execCommand("cut");
      return;
    }
    if (key === "v") {
      e.preventDefault();
      try {
        if (navigator.clipboard && navigator.clipboard.readText) {
          const text = await navigator.clipboard.readText();
          insertText(text);
          return;
        }
      } catch (_) {}
      try { document.execCommand("paste"); } catch (_) {}
    }
  }, true);
})();
"#;
#[cfg(not(target_arch = "wasm32"))]
const WEBVIEW_IMAGE_OVERLAY_JS: &str = r#"
(() => {
  if (window.__kaspaNgImageOverlay) return;
  window.__kaspaNgImageOverlay = true;

  const OVERLAY_ID = "__kaspa_ng_img_overlay";
  const IMG_ID = "__kaspa_ng_img_overlay_img";
  const CLOSE_ID = "__kaspa_ng_img_overlay_close";
  const ERROR_ID = "__kaspa_ng_img_overlay_error";
  const LOADER_ID = "__kaspa_ng_img_overlay_loader";

  const IMAGE_EXTENSIONS = [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg", ".avif"];

  const isAllowedProtocol = (value) => (
    value.startsWith("http://") ||
    value.startsWith("https://") ||
    value.startsWith("data:image/") ||
    value.startsWith("blob:")
  );

  const looksLikeImageHref = (href) => {
    const lower = href.toLowerCase();
    if (IMAGE_EXTENSIONS.some((ext) => lower.includes(ext))) return true;
    return (
      lower.includes("format=jpg") ||
      lower.includes("format=jpeg") ||
      lower.includes("format=png") ||
      lower.includes("format=webp") ||
      lower.includes("format=gif") ||
      lower.includes("image/")
    );
  };

  const ensureOverlay = () => {
    let overlay = document.getElementById(OVERLAY_ID);
    if (overlay) return overlay;

    const style = document.createElement("style");
    style.id = "__kaspa_ng_img_overlay_style";
    style.textContent = `
      #${OVERLAY_ID} {
        position: fixed;
        inset: 0;
        z-index: 2147483647;
        display: none;
        align-items: center;
        justify-content: center;
        background: rgba(0, 0, 0, 0.88);
        padding: 24px;
        box-sizing: border-box;
      }
      #${OVERLAY_ID}[data-open="1"] {
        display: flex;
      }
      #${IMG_ID} {
        max-width: min(96vw, 1800px);
        max-height: 92vh;
        width: auto;
        height: auto;
        object-fit: contain;
        border-radius: 8px;
        box-shadow: 0 18px 48px rgba(0, 0, 0, 0.5);
        background: rgba(10, 10, 10, 0.9);
      }
      #${CLOSE_ID} {
        position: absolute;
        top: 14px;
        right: 16px;
        border: 0;
        border-radius: 6px;
        color: #f5f5f5;
        background: rgba(20, 20, 20, 0.72);
        padding: 8px 10px;
        line-height: 1;
        font-size: 26px;
        cursor: pointer;
      }
      #${ERROR_ID}, #${LOADER_ID} {
        position: absolute;
        bottom: 16px;
        left: 50%;
        transform: translateX(-50%);
        color: #efefef;
        background: rgba(20, 20, 20, 0.78);
        border-radius: 6px;
        padding: 8px 10px;
        font-size: 13px;
        font-family: sans-serif;
      }
      #${ERROR_ID} { display: none; color: #ffc9c9; }
      #${LOADER_ID} { display: none; }
      @media (max-width: 640px) {
        #${OVERLAY_ID} { padding: 10px; }
      }
    `;
    document.head.appendChild(style);

    overlay = document.createElement("div");
    overlay.id = OVERLAY_ID;
    overlay.setAttribute("role", "dialog");
    overlay.setAttribute("aria-modal", "true");
    overlay.setAttribute("aria-label", "Image preview");

    const close = document.createElement("button");
    close.id = CLOSE_ID;
    close.type = "button";
    close.setAttribute("aria-label", "Close image preview");
    close.textContent = "x";

    const img = document.createElement("img");
    img.id = IMG_ID;
    img.alt = "Preview";

    const loader = document.createElement("div");
    loader.id = LOADER_ID;
    loader.textContent = "Loading image...";

    const error = document.createElement("div");
    error.id = ERROR_ID;
    error.textContent = "Image failed to load";

    overlay.appendChild(close);
    overlay.appendChild(img);
    overlay.appendChild(loader);
    overlay.appendChild(error);
    document.body.appendChild(overlay);

    const closeOverlay = () => {
      overlay.setAttribute("data-open", "0");
      img.removeAttribute("src");
      error.style.display = "none";
      loader.style.display = "none";
      document.body.style.overflow = "";
    };

    close.addEventListener("click", closeOverlay);
    overlay.addEventListener("click", (event) => {
      if (event.target === overlay) closeOverlay();
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && overlay.getAttribute("data-open") === "1") {
        closeOverlay();
      }
    });

    img.addEventListener("load", () => {
      loader.style.display = "none";
      error.style.display = "none";
    });
    img.addEventListener("error", () => {
      loader.style.display = "none";
      error.style.display = "block";
    });

    overlay.__kaspaNgOpen = (src) => {
      loader.style.display = "block";
      error.style.display = "none";
      overlay.setAttribute("data-open", "1");
      document.body.style.overflow = "hidden";
      img.src = src;
    };

    return overlay;
  };

  const normalize = (value) => {
    try {
      return new URL(value, window.location.href).toString();
    } catch (_) {
      return "";
    }
  };

  const findImageSource = (event) => {
    const target = event.target instanceof Element ? event.target : null;
    if (!target) return "";

    const image = target.closest("img");
    if (image && image.src) {
      const src = normalize(image.src);
      if (src && isAllowedProtocol(src)) return src;
    }

    const anchor = target.closest("a");
    if (!anchor) return "";
    const href = (anchor.getAttribute("href") || "").trim();
    if (!href || href.startsWith("javascript:")) return "";
    const url = normalize(href);
    if (!url || !isAllowedProtocol(url)) return "";
    if (anchor.querySelector("img")) return url;
    if (looksLikeImageHref(url)) return url;
    return "";
  };

  document.addEventListener("click", (event) => {
    const source = findImageSource(event);
    if (!source) return;
    event.preventDefault();
    event.stopPropagation();
    const overlay = ensureOverlay();
    if (overlay && overlay.__kaspaNgOpen) overlay.__kaspaNgOpen(source);
  }, true);
})();
"#;


#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
fn embedded_explorer_enabled() -> bool {
    std::env::var("KASPA_NG_EMBEDDED_EXPLORER")
        .map(|v| !matches!(v.as_str(), "0" | "false" | "FALSE" | "no" | "NO"))
        .unwrap_or(true)
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


pub struct Explorer {
    #[allow(dead_code)]
    runtime: Runtime,
    #[cfg(not(target_arch = "wasm32"))]
    server: Option<ExplorerServer>,
    #[cfg(not(target_arch = "wasm32"))]
    webview: Option<WebView>,
    #[cfg(not(target_arch = "wasm32"))]
    last_bounds: Option<WryRect>,
    #[cfg(not(target_arch = "wasm32"))]
    last_zoom: Option<f64>,
    #[cfg(not(target_arch = "wasm32"))]
    last_path: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    last_endpoint_signature: Option<(ExplorerDataSource, Network, String, String, String)>,
    #[cfg(not(target_arch = "wasm32"))]
    status: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    last_webview_attempt: Option<std::time::Instant>,
    #[cfg(not(target_arch = "wasm32"))]
    last_server_start_attempt: Option<std::time::Instant>,
}

impl Explorer {
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
            last_path: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_endpoint_signature: None,
            #[cfg(not(target_arch = "wasm32"))]
            status: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_webview_attempt: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_server_start_attempt: None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn reset_embedded_state(&mut self) {
        self.webview.take();
        self.server.take();
        self.last_bounds = None;
        self.last_zoom = None;
        self.last_path = None;
        self.last_endpoint_signature = None;
        self.last_webview_attempt = None;
        self.last_server_start_attempt = None;
        self.status = None;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_server_for_active_network(&mut self, core: &Core) {
        let expected_port = core
            .settings
            .user_interface
            .effective_explorer_port(core.settings.node.network);

        let server_port_mismatch = self
            .server
            .as_ref()
            .map(|server| server.port != expected_port)
            .unwrap_or(false);

        if server_port_mismatch {
            self.reset_embedded_state();
        }

        if self.server.is_none() {
            if let Some(last_attempt) = self.last_server_start_attempt
                && last_attempt.elapsed() < std::time::Duration::from_secs(1)
            {
                return;
            }
            self.last_server_start_attempt = Some(std::time::Instant::now());
            match ExplorerServer::start(expected_port) {
                Ok(server) => {
                    self.server = Some(server);
                    self.status = None;
                    self.last_server_start_attempt = None;
                }
                Err(err) => {
                    log_warn!("Explorer server start failed: {err}");
                    self.status = Some(err);
                }
            }
        }
    }
}

impl ModuleT for Explorer {
    fn name(&self) -> Option<&'static str> {
        Some(i18n("Explorer"))
    }

    fn activate(&mut self, core: &mut Core) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.ensure_server_for_active_network(core);

            if let Some(webview) = &self.webview {
                let _ = webview.set_visible(true);
                let _ = webview.focus();
            }
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

    fn network_change(&mut self, _core: &mut Core, _network: Network) {
        #[cfg(not(target_arch = "wasm32"))]
        self.reset_embedded_state();
    }

    fn render(
        &mut self,
        core: &mut Core,
        _ctx: &egui::Context,
        frame: &mut eframe::Frame,
        ui: &mut egui::Ui,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.ensure_server_for_active_network(core);
            let mut effective_source = core.settings.explorer.source;
            if matches!(core.settings.explorer.source, ExplorerDataSource::SelfHosted) {
                let loader = runtime().self_hosted_loader_service().status_snapshot();
                if !loader.connected {
                    effective_source = ExplorerDataSource::Official;
                }
            }
            let endpoint = match effective_source {
                ExplorerDataSource::Official => core
                    .settings
                    .explorer
                    .official
                    .for_network(core.settings.node.network)
                    .clone(),
                ExplorerDataSource::SelfHosted => self_hosted_explorer_profiles_from_settings(
                    &core.settings.self_hosted,
                )
                .for_network(core.settings.node.network)
                .clone(),
            };
            let node_display = endpoint.socket_url.trim().to_string();
            set_footer_connection_status(FooterConnectionStatus {
                node: node_display.clone(),
                node_health: if node_display.is_empty() {
                    FooterConnectionHealth::Unknown
                } else {
                    FooterConnectionHealth::Reachable
                },
                api: "unknown".to_string(),
                api_health: FooterConnectionHealth::Unknown,
            });

            #[cfg(target_os = "linux")]
            {
                if let Err(err) = ensure_gtk_initialized() {
                    self.status = Some(format!("GTK init failed: {err}"));
                    ui.colored_label(theme_color().error_color, i18n("Explorer is unavailable."));
                    return;
                }
                while gtk::events_pending() {
                    gtk::main_iteration_do(false);
                }
            }

            if let Some(status) = &self.status {
                ui.colored_label(theme_color().error_color, status);
            }

            if let Some(server) = &self.server {
                #[cfg(target_os = "linux")]
                if !embedded_explorer_enabled() {
                    ui.label(i18n(
                        "Embedded Explorer is disabled via KASPA_NG_EMBEDDED_EXPLORER.",
                    ));
                    return;
                }

                let available_rect = ui.available_rect_before_wrap();
                ui.allocate_rect(available_rect, Sense::hover());
                let bounds = webview_bounds_from_rect(available_rect, _ctx.pixels_per_point());
                let target_zoom = f64::from(_ctx.zoom_factor().max(0.5));

                let endpoint_signature = Some((
                    effective_source,
                    core.settings.node.network,
                    endpoint.api_base.clone(),
                    endpoint.socket_url.clone(),
                    endpoint.socket_path.clone(),
                ));
                let node_display = node_display.clone();
                let node_health = if core.state().is_connected() {
                    FooterConnectionHealth::Connected
                } else if node_display.is_empty() {
                    FooterConnectionHealth::Unknown
                } else {
                    FooterConnectionHealth::Reachable
                };
                let api_health = if self.status.is_some() {
                    FooterConnectionHealth::Unreachable
                } else if self.webview.is_some() {
                    FooterConnectionHealth::Connected
                } else {
                    FooterConnectionHealth::Reachable
                };
                set_footer_connection_status(FooterConnectionStatus {
                    node: node_display.clone(),
                    node_health,
                    api: endpoint.api_base.clone(),
                    api_health,
                });
                if self.webview.is_some() && self.last_endpoint_signature != endpoint_signature {
                    self.webview.take();
                    self.last_bounds = None;
                    self.last_zoom = None;
                }

                if self.webview.is_none() {
                    if let Some(last_attempt) = self.last_webview_attempt {
                        if last_attempt.elapsed() < std::time::Duration::from_secs(2) {
                            return;
                        }
                    }
                    self.last_webview_attempt = Some(std::time::Instant::now());

                    let start_url = explorer_start_url(
                        server,
                        &core.settings.user_interface.explorer_last_path,
                        &endpoint,
                        core.settings.node.network,
                    );
                    let config_script = explorer_runtime_config_script(
                        &endpoint,
                        core.settings.node.network,
                        effective_source,
                        node_display.as_str(),
                    );
                    match WebViewBuilder::new()
                        .with_url(start_url.as_str())
                        .with_bounds(bounds)
                        .with_clipboard(true)
                        .with_accept_first_mouse(true)
                        .with_focused(true)
                        .with_initialization_script(WEBVIEW_SHORTCUTS_JS)
                        .with_initialization_script(WEBVIEW_IMAGE_OVERLAY_JS)
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
                            self.last_path = Some(core.settings.user_interface.explorer_last_path.clone());
                            self.last_endpoint_signature = endpoint_signature;
                            self.status = None;
                        }
                        Err(err) => {
                            #[cfg(target_os = "linux")]
                            {
                                self.status = Some(format!(
                                    "Explorer WebView error: {err}. Ensure webkit2gtk is installed (e.g. libwebkit2gtk-4.1-dev or webkit2gtk4.0-devel).",
                                ));
                            }
                            #[cfg(not(target_os = "linux"))]
                            {
                                self.status = Some(format!("Explorer WebView error: {err}"));
                            }
                            return;
                        }
                    }
                }

                if let Some(webview) = &mut self.webview {
                    let focus_requested = _ctx.input(|input| {
                        input.pointer.any_pressed()
                            && input
                                .pointer
                                .latest_pos()
                                .map(|pos| available_rect.contains(pos))
                                .unwrap_or(false)
                    });

                    if focus_requested {
                        let _ = webview.focus();
                    }

                    let (copy, cut, paste) = _ctx.input(|input| {
                        let cmd = input.modifiers.command;
                        (
                            cmd && input.key_pressed(egui::Key::C),
                            cmd && input.key_pressed(egui::Key::X),
                            cmd && input.key_pressed(egui::Key::V),
                        )
                    });

                    if copy {
                        let _ = webview.evaluate_script(WEBVIEW_COPY_JS);
                    }
                    if cut {
                        let _ = webview.evaluate_script(WEBVIEW_CUT_JS);
                    }
                    if paste {
                        let _ = webview.evaluate_script(WEBVIEW_PASTE_JS);
                    }

                    if let Ok(current_url) = webview.url() {
                        if let Some(path) = extract_explorer_path(&current_url, server.url.as_str()) {
                            if self.last_path.as_ref() != Some(&path) {
                                self.last_path = Some(path.clone());
                                if core.settings.user_interface.explorer_last_path != path {
                                    core.settings.user_interface.explorer_last_path = path;
                                    core.store_settings();
                                }
                            }
                        }
                    }

                    if self.last_bounds != Some(bounds) {
                        if let Err(err) = webview.set_bounds(bounds) {
                            self.status = Some(format!("Explorer WebView resize error: {err}"));
                            return;
                        }
                        self.last_bounds = Some(bounds);
                    }
                    if self
                        .last_zoom
                        .map(|zoom| (zoom - target_zoom).abs() > 0.001)
                        .unwrap_or(true)
                    {
                        if let Err(err) = webview.zoom(target_zoom) {
                            self.status = Some(format!("Explorer WebView zoom error: {err}"));
                            return;
                        }
                        self.last_zoom = Some(target_zoom);
                    }
                }
            } else {
                ui.label(i18n("Starting explorer..."));
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            ui.label(i18n("Explorer is not available in Web builds."));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct ExplorerServer {
    url: String,
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ExplorerServer {
    fn start(port: u16) -> std::result::Result<Self, String> {
        let root = find_explorer_root().ok_or_else(|| {
            i18n("Explorer build not found. Run `npm install` and `npm run build` in `kaspa-explorer-ng`.")
                .to_string()
        })?;

        let port = if port == 0 { DEFAULT_EXPLORER_PORT } else { port };
        let listener = TcpListener::bind((EXPLORER_HOST, port)).map_err(|err| {
            format!(
                "Explorer server bind failed on {}:{} ({err}). Close other instances or free the port.",
                EXPLORER_HOST, port
            )
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|err| format!("Explorer server nonblocking setup failed: {err}"))?;
        let port = listener
            .local_addr()
            .map_err(|err| format!("Explorer server address error: {err}"))?
            .port();
        let url = format!("http://{EXPLORER_HOST}:{port}/");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_signal = Arc::clone(&stop);

        let thread = std::thread::Builder::new()
            .name("kaspa-explorer-server".to_string())
            .spawn(move || {
                while !stop_signal.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let root = root.clone();
                            std::thread::spawn(move || {
                                let _ = handle_connection(stream, &root);
                            });
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|err| format!("Explorer server spawn failed: {err}"))?;

        Ok(Self {
            url,
            port,
            stop,
            thread: Some(thread),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for ExplorerServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn find_explorer_root() -> Option<PathBuf> {
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
    if let Ok(root) = std::env::var("KASPA_NG_EXPLORER_ROOT") {
        let root = PathBuf::from(root);
        candidates.push(root.join("build").join("client"));
        candidates.push(root.join("build"));
        candidates.push(root.join("dist"));
    }
    if !is_macos_bundle && let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("kaspa-explorer-ng").join("build").join("client"));
        candidates.push(cwd.join("kaspa-explorer-ng").join("build"));
        candidates.push(cwd.join("kaspa-explorer-ng").join("dist"));
        for ancestor in cwd.ancestors().skip(1).take(3) {
            candidates.push(ancestor.join("kaspa-explorer-ng").join("build").join("client"));
            candidates.push(ancestor.join("kaspa-explorer-ng").join("build"));
            candidates.push(ancestor.join("kaspa-explorer-ng").join("dist"));
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("kaspa-explorer-ng").join("build").join("client"));
            candidates.push(dir.join("kaspa-explorer-ng").join("build"));
            candidates.push(dir.join("kaspa-explorer-ng").join("dist"));
            if is_macos_bundle {
                if let Some(contents) = dir.parent() {
                    let resources = contents.join("Resources").join("kaspa-explorer-ng");
                    candidates.push(resources.join("build").join("client"));
                    candidates.push(resources.join("build"));
                    candidates.push(resources.join("dist"));
                }
            } else {
                for ancestor in dir.ancestors().skip(1).take(4) {
                    candidates.push(ancestor.join("kaspa-explorer-ng").join("build").join("client"));
                    candidates.push(ancestor.join("kaspa-explorer-ng").join("build"));
                    candidates.push(ancestor.join("kaspa-explorer-ng").join("dist"));
                }
            }
        }
    }

    candidates
        .into_iter()
        .find(|dir| dir.join("index.html").exists())
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_connection(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut buf = [0_u8; 8192];
    let read = stream.read(&mut buf)?;
    if read == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buf[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let path = sanitize_path(path);

    let mut status = "200 OK";
    let mut file_path = root.join("index.html");

    if path != "/" {
        let requested_path = root.join(path.trim_start_matches('/'));
        if requested_path.exists() && requested_path.is_file() {
            file_path = requested_path;
        } else {
            let route_like_path = Path::new(path.trim_start_matches('/'))
                .extension()
                .is_none();
            if !route_like_path {
                status = "404 Not Found";
            }
        }
    }

    let body = if status == "404 Not Found" {
        b"Not Found".to_vec()
    } else {
        match std::fs::read(&file_path) {
            Ok(bytes) => bytes,
            Err(_) => {
                status = "500 Internal Server Error";
                b"Internal Server Error".to_vec()
            }
        }
    };

    let content_type = match status {
        "404 Not Found" | "500 Internal Server Error" => "text/plain; charset=utf-8",
        _ => content_type_for_path(&file_path),
    };

    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store, no-cache, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nConnection: close\r\n\r\n",
        body.len()
    );

    stream.write_all(header.as_bytes())?;
    if !body.is_empty() {
        stream.write_all(&body)?;
    }
    stream.flush()?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn sanitize_path(path: &str) -> String {
    let path = path.split('?').next().unwrap_or(path);
    let decoded = percent_decode(path);
    let cleaned = decoded
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
        .collect::<Vec<_>>()
        .join("/");
    format!("/{}", cleaned)
}

#[cfg(not(target_arch = "wasm32"))]
fn percent_decode(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut chars = path.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let hi = chars.next();
            let lo = chars.next();
            if let (Some(hi), Some(lo)) = (hi, lo) {
                if let Ok(byte) = u8::from_str_radix(&format!("{hi}{lo}"), 16) {
                    out.push(byte as char);
                    continue;
                }
            }
            out.push(ch);
            if let Some(hi) = hi {
                out.push(hi);
            }
            if let Some(lo) = lo {
                out.push(lo);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn explorer_start_url(
    server: &ExplorerServer,
    stored_path: &str,
    endpoint: &ExplorerEndpoint,
    network: Network,
) -> String {
    let trimmed = stored_path.trim();
    let trimmed = trimmed
        .split('#')
        .next()
        .unwrap_or(trimmed)
        .split('?')
        .next()
        .unwrap_or(trimmed);
    let base = if trimmed.is_empty() || trimmed == "/" {
        server.url.clone()
    } else {
        let path = trimmed.strip_prefix('/').unwrap_or(trimmed);
        format!("{}{}", server.url, path)
    };

    let separator = if base.contains('?') { '&' } else { '?' };
    format!(
        "{base}{separator}apiBase={}&socketUrl={}&socketPath={}&networkId={}",
        query_escape(endpoint.api_base.as_str()),
        query_escape(endpoint.socket_url.as_str()),
        query_escape(endpoint.socket_path.as_str()),
        query_escape(network.to_string().as_str())
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn query_escape(value: &str) -> String {
    value
        .bytes()
        .flat_map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![b as char]
            }
            _ => {
                let hex = format!("%{b:02X}");
                hex.chars().collect()
            }
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn extract_explorer_path(current_url: &str, base_url: &str) -> Option<String> {
    if !current_url.starts_with(base_url) {
        return None;
    }
    let remainder = current_url.trim_start_matches(base_url);
    if remainder.is_empty() {
        return Some("/".to_string());
    }
    let remainder = remainder
        .split('#')
        .next()
        .unwrap_or(remainder)
        .split('?')
        .next()
        .unwrap_or(remainder);
    if remainder.starts_with('/') {
        Some(remainder.to_string())
    } else {
        Some(format!("/{remainder}"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn js_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn explorer_runtime_config_script(
    endpoint: &ExplorerEndpoint,
    network: Network,
    source: ExplorerDataSource,
    _node_url: &str,
) -> String {
    let source = match source {
        ExplorerDataSource::Official => "official",
        ExplorerDataSource::SelfHosted => "self-hosted",
    };
    format!(
        "window.__KASPA_EXPLORER_CONFIG__={{apiBase:{},socketUrl:{},socketPath:{},networkId:{},apiSource:{}}};",
        js_quote(endpoint.api_base.as_str()),
        js_quote(endpoint.socket_url.as_str()),
        js_quote(endpoint.socket_path.as_str()),
        js_quote(network.to_string().as_str()),
        js_quote(source),
    )
}
