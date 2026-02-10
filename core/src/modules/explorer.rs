use crate::imports::*;

#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpListener, TcpStream};
#[cfg(not(target_arch = "wasm32"))]
use std::thread::JoinHandle;

#[cfg(not(target_arch = "wasm32"))]
use wry::{dpi::LogicalPosition, dpi::LogicalSize, Rect as WryRect, WebView, WebViewBuilder};

#[cfg(not(target_arch = "wasm32"))]
const EXPLORER_HOST: &str = "127.0.0.1";
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_EXPLORER_PORT: u16 = 51963;
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
    last_path: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    status: Option<String>,
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
            last_path: None,
            #[cfg(not(target_arch = "wasm32"))]
            status: None,
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
            if self.server.is_none() && self.status.is_none() {
                let port = core.settings.user_interface.explorer_port;
                match ExplorerServer::start(port) {
                    Ok(server) => {
                        self.server = Some(server);
                    }
                    Err(err) => {
                        self.status = Some(err);
                    }
                }
            }

            if let Some(webview) = &self.webview {
                let _ = webview.set_visible(true);
                let _ = webview.focus();
            }
        }
    }

    fn deactivate(&mut self, _core: &mut Core) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(webview) = &self.webview {
            let _ = webview.set_visible(false);
        }
    }

    fn hide(&mut self, _core: &mut Core) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(webview) = &self.webview {
            let _ = webview.set_visible(false);
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
        _ctx: &egui::Context,
        frame: &mut eframe::Frame,
        ui: &mut egui::Ui,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(status) = &self.status {
                ui.colored_label(theme_color().error_color, status);
                return;
            }

            if let Some(server) = &self.server {
                let available_rect = ui.available_rect_before_wrap();
                ui.allocate_rect(available_rect, Sense::hover());

                let bounds = WryRect {
                    position: LogicalPosition::new(
                        available_rect.min.x as f64,
                        available_rect.min.y as f64,
                    )
                    .into(),
                    size: LogicalSize::new(
                        available_rect.width() as f64,
                        available_rect.height() as f64,
                    )
                    .into(),
                };

                if self.webview.is_none() {
                    let start_url = explorer_start_url(server, &core.settings.user_interface.explorer_last_path);
                    match WebViewBuilder::new()
                        .with_url(start_url.as_str())
                        .with_bounds(bounds)
                        .with_clipboard(true)
                        .with_accept_first_mouse(true)
                        .with_focused(true)
                        .with_initialization_script(WEBVIEW_SHORTCUTS_JS)
                        .build_as_child(frame)
                    {
                        Ok(webview) => {
                            let _ = webview.focus();
                            self.webview = Some(webview);
                            self.last_bounds = Some(bounds);
                            self.last_path = Some(core.settings.user_interface.explorer_last_path.clone());
                        }
                        Err(err) => {
                            self.status = Some(format!("Explorer WebView error: {err}"));
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
    _thread: JoinHandle<()>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ExplorerServer {
    fn start(port: u16) -> std::result::Result<Self, String> {
        let root = find_explorer_root().ok_or_else(|| {
            i18n("Explorer build not found. Run `npm install` and `npm run build` in `kaspa-explorer-ng`.")
                .to_string()
        })?;

        let port = if port == 0 { DEFAULT_EXPLORER_PORT } else { port };
        let listener = TcpListener::bind((EXPLORER_HOST, port))
            .map_err(|err| {
                format!(
                    "Explorer server bind failed on {}:{} ({err}). Close other instances or free the port.",
                    EXPLORER_HOST, port
                )
            })?;
        let port = listener
            .local_addr()
            .map_err(|err| format!("Explorer server address error: {err}"))?
            .port();
        let url = format!("http://{EXPLORER_HOST}:{port}/");

        let thread = std::thread::Builder::new()
            .name("kaspa-explorer-server".to_string())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let root = root.clone();
                    std::thread::spawn(move || {
                        let _ = handle_connection(stream, &root);
                    });
                }
            })
            .map_err(|err| format!("Explorer server spawn failed: {err}"))?;

        Ok(Self {
            url,
            _thread: thread,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn find_explorer_root() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("kaspa-explorer-ng").join("build").join("client"));
        candidates.push(cwd.join("kaspa-explorer-ng").join("build"));
        candidates.push(cwd.join("kaspa-explorer-ng").join("dist"));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("kaspa-explorer-ng").join("build").join("client"));
            candidates.push(dir.join("kaspa-explorer-ng").join("build"));
            candidates.push(dir.join("kaspa-explorer-ng").join("dist"));
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

    let mut file_path = root.join(path.trim_start_matches('/'));
    if path == "/" || !file_path.exists() || !file_path.is_file() {
        file_path = root.join("index.html");
    }

    let body = std::fs::read(&file_path).unwrap_or_default();
    let content_type = content_type_for_path(&file_path);

    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\n\r\n",
        body.len()
    );

    stream.write_all(header.as_bytes())?;
    stream.write_all(&body)?;
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
fn explorer_start_url(server: &ExplorerServer, stored_path: &str) -> String {
    let trimmed = stored_path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return server.url.clone();
    }
    let path = trimmed.strip_prefix('/').unwrap_or(trimmed);
    format!("{}{}", server.url, path)
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
    if remainder.starts_with('/') {
        Some(remainder.to_string())
    } else {
        Some(format!("/{remainder}"))
    }
}
