use crate::imports::*;
use std::collections::VecDeque;

#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
#[cfg(not(target_arch = "wasm32"))]
use std::thread::JoinHandle;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};

#[cfg(not(target_arch = "wasm32"))]
use wry::{dpi::LogicalPosition, dpi::LogicalSize, Rect as WryRect, WebView, WebViewBuilder};

#[cfg(not(target_arch = "wasm32"))]
const K_HOST: &str = "127.0.0.1";
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_K_PORT: u16 = 51964;
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
})();
"#;

pub struct KSocial {
    #[allow(dead_code)]
    runtime: Runtime,
    #[cfg(not(target_arch = "wasm32"))]
    server: Option<KSocialServer>,
    #[cfg(not(target_arch = "wasm32"))]
    webview: Option<WebView>,
    #[cfg(not(target_arch = "wasm32"))]
    last_bounds: Option<WryRect>,
    #[cfg(not(target_arch = "wasm32"))]
    last_signature: Option<(Network, String, u16)>,
    #[cfg(not(target_arch = "wasm32"))]
    status: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    last_probe_at: Option<Instant>,
    #[cfg(not(target_arch = "wasm32"))]
    last_probe_ok: Option<bool>,
    #[cfg(not(target_arch = "wasm32"))]
    last_probe_status: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    waiting_since: Option<Instant>,
    logs: VecDeque<String>,
}

impl KSocial {
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
            waiting_since: None,
            logs: VecDeque::new(),
        }
    }

    fn push_log(&mut self, message: impl Into<String>) {
        const MAX_LOG_LINES: usize = 200;
        self.logs.push_back(message.into());
        while self.logs.len() > MAX_LOG_LINES {
            let _ = self.logs.pop_front();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_local_server(&mut self, api_host: &str, api_port: u16) {
        if self
            .server
            .as_ref()
            .is_some_and(|server| server.api_host == api_host && server.api_port == api_port)
        {
            return;
        }
        self.server.take();
        match KSocialServer::start(DEFAULT_K_PORT, api_host.to_string(), api_port) {
            Ok(server) => {
                self.push_log(format!(
                    "K-Social: local web server started on {}:{} (api {}:{})",
                    server.host, server.port, server.api_host, server.api_port
                ));
                self.server = Some(server);
            }
            Err(err) => {
                log_warn!("K-Social server start failed: {err}");
                self.status = Some(err);
                self.push_log("K-Social: local web server failed to start");
            }
        }
    }
}

impl ModuleT for KSocial {
    fn name(&self) -> Option<&'static str> {
        Some(i18n("K-Social"))
    }

    fn activate(&mut self, core: &mut Core) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.push_log("K-Social: activate requested");
            if !matches!(core.settings.node.network, Network::Mainnet) {
                self.webview.take();
                self.server.take();
                self.last_bounds = None;
                self.last_signature = None;
                self.status = Some(i18n("K-Social is available only on Mainnet.").to_string());
                self.last_probe_status = None;
                self.waiting_since = None;
                self.push_log("K-Social: blocked (Mainnet only)");
                return;
            }

            if !core.settings.self_hosted.enabled || !core.settings.self_hosted.k_enabled {
                self.status = Some(i18n("K-Social is disabled in Settings.").to_string());
                self.last_probe_status = None;
                self.waiting_since = None;
                self.push_log("K-Social: blocked (disabled in Settings)");
                return;
            }

            let host = if core.settings.self_hosted.api_bind == "0.0.0.0"
                || core.settings.self_hosted.api_bind == "::"
                || core.settings.self_hosted.api_bind == "[::]"
            {
                "127.0.0.1".to_string()
            } else {
                core.settings.self_hosted.api_bind.clone()
            };
            let indexer_port = core.settings.self_hosted.k_web_port;
            self.status = None;
            self.ensure_local_server(&host, indexer_port);

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
            if !matches!(core.settings.node.network, Network::Mainnet) {
                ui.label(i18n("K-Social is available only on Mainnet."));
                return;
            }

            if let Some(status) = &self.status {
                ui.colored_label(theme_color().error_color, status);
            }

            let host = if core.settings.self_hosted.api_bind == "0.0.0.0"
                || core.settings.self_hosted.api_bind == "::"
                || core.settings.self_hosted.api_bind == "[::]"
            {
                "127.0.0.1".to_string()
            } else {
                core.settings.self_hosted.api_bind.clone()
            };
            let indexer_port = core.settings.self_hosted.k_web_port;
            let mut k_api_ready = false;

            if core.settings.self_hosted.enabled && core.settings.self_hosted.k_enabled {
                let should_probe = self
                    .last_probe_at
                    .map(|last| last.elapsed() >= Duration::from_secs(2))
                    .unwrap_or(true);
                if should_probe {
                    let probe = k_indexer_health(&host, indexer_port);
                    let probe_ok = probe.ready;
                    let probe_status = probe.status;
                    if self.last_probe_ok != Some(probe_ok) {
                        if probe_ok {
                            log_info!(
                                "K-Social: K-indexer API reachable at http://{}:{}/health",
                                host,
                                indexer_port
                            );
                            self.push_log(format!(
                                "K-Social: K-indexer API reachable at http://{}:{}/health",
                                host, indexer_port
                            ));
                            self.waiting_since = None;
                        } else {
                            log_warn!(
                                "K-Social: K-indexer API is not reachable at http://{}:{}/health",
                                host,
                                indexer_port
                            );
                            self.push_log(format!(
                                "K-Social: waiting for K-indexer API at http://{}:{}/health",
                                host, indexer_port
                            ));
                            self.waiting_since.get_or_insert_with(Instant::now);
                        }
                    } else if !probe_ok {
                        self.waiting_since.get_or_insert_with(Instant::now);
                    }
                    self.last_probe_ok = Some(probe_ok);
                    self.last_probe_status = Some(probe_status);
                    self.last_probe_at = Some(Instant::now());
                }

                k_api_ready = matches!(self.last_probe_ok, Some(true));
            } else {
                self.last_probe_ok = None;
                self.last_probe_status = None;
                self.last_probe_at = None;
                self.waiting_since = None;
            }

            if !core.settings.self_hosted.enabled || !core.settings.self_hosted.k_enabled {
                ui.label(i18n("Enable K-Social services in Settings."));
            } else if !k_api_ready {
                let waited = self
                    .waiting_since
                    .map(|since| since.elapsed().as_secs())
                    .unwrap_or_default();
                ui.label(format!(
                    "{} http://{}:{}/health ({}s)",
                    i18n("Waiting for K-indexer API:"),
                    host,
                    indexer_port,
                    waited
                ));
                if waited >= 30 {
                    ui.colored_label(
                        theme_color().warning_color,
                        i18n("K-indexer is still initializing. K-Social will auto-attach when ready."),
                    );
                }
            }

            if !core.settings.self_hosted.enabled || !core.settings.self_hosted.k_enabled || !k_api_ready {
                return;
            }

            self.ensure_local_server(&host, indexer_port);
            if self.server.is_none() {
                ui.colored_label(
                    theme_color().error_color,
                    i18n("K-Social local web server is not available."),
                );
                return;
            }

            if let Some((server_host, server_port)) =
                self.server.as_ref().map(|server| (server.host.clone(), server.port))
            {
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

                let signature = Some((core.settings.node.network, host.clone(), indexer_port));
                if self.webview.is_some() && self.last_signature != signature {
                    self.webview.take();
                    self.last_bounds = None;
                }

                if self.webview.is_none() {
                    let start_url = format!("http://{}:{}/", server_host, server_port);
                    let kaspa_node_url = runtime()
                        .kaspa_service()
                        .rpc_url()
                        .map(|url| normalize_k_node_url(url))
                        .unwrap_or_else(|| "ws://127.0.0.1:17110".to_string());
                    let config_script = k_runtime_config_script(
                        &host,
                        indexer_port,
                        core.settings.node.network,
                        &kaspa_node_url,
                    );
                    log_info!(
                        "K-Social: loading web app from {} with K-indexer API http://{}:{}",
                        start_url,
                        host,
                        indexer_port
                    );
                    self.push_log(format!(
                        "K-Social: loading web app with API http://{}:{}",
                        host, indexer_port
                    ));

                    match WebViewBuilder::new()
                        .with_url(start_url.as_str())
                        .with_bounds(bounds)
                        .with_clipboard(true)
                        .with_accept_first_mouse(true)
                        .with_focused(true)
                        .with_initialization_script(WEBVIEW_SHORTCUTS_JS)
                        .with_initialization_script(config_script.as_str())
                        .build_as_child(frame)
                    {
                        Ok(webview) => {
                            let _ = webview.set_visible(true);
                            let _ = webview.focus();
                            self.webview = Some(webview);
                            self.last_bounds = Some(bounds);
                            self.last_signature = signature;
                            self.status = None;
                            self.waiting_since = None;
                            self.push_log("K-Social: WebView attached");
                        }
                        Err(err) => {
                            self.status = Some(format!("K-Social WebView error: {err}"));
                            self.push_log(format!("K-Social: WebView error ({err})"));
                        }
                    }
                } else if let Some(webview) = &self.webview {
                    let mut resize_error: Option<String> = None;
                    if self.last_bounds != Some(bounds) {
                        if let Err(err) = webview.set_bounds(bounds) {
                            resize_error = Some(err.to_string());
                        } else {
                            self.last_bounds = Some(bounds);
                        }
                    }
                    let _ = webview.set_visible(true);
                    if let Some(err) = resize_error {
                        self.status = Some(format!("K-Social WebView resize error: {err}"));
                        self.push_log(format!("K-Social: WebView resize error ({err})"));
                    }
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let _ = core;
            let _ = frame;
            ui.label(i18n("K-Social is not available in Web builds."));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct KSocialServer {
    host: String,
    port: u16,
    api_host: String,
    api_port: u16,
    stop_tx: std::sync::mpsc::Sender<()>,
    join: Option<JoinHandle<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl KSocialServer {
    fn start(port: u16, api_host: String, api_port: u16) -> std::result::Result<Self, String> {
        let root = find_k_build_root().ok_or_else(|| {
            i18n("K build not found. Run `npm install` and `npm run build` in `K`.")
        })?;

        let listener = TcpListener::bind((K_HOST, port))
            .map_err(|err| format!("K-Social server bind failed on {K_HOST}:{port} ({err})"))?;
        listener
            .set_nonblocking(true)
            .map_err(|err| format!("K-Social server nonblocking setup failed: {err}"))?;
        let addr = listener
            .local_addr()
            .map_err(|err| format!("K-Social server address error: {err}"))?;
        let host = addr.ip().to_string();
        let port = addr.port();
        let thread_api_host = api_host.clone();

        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let join = std::thread::Builder::new()
            .name("k-social-server".to_string())
            .spawn(move || loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }

                match listener.accept() {
                    Ok((stream, _)) => {
                        let root = root.clone();
                        let api_host = thread_api_host.clone();
                        let _ = std::thread::Builder::new()
                            .name("k-social-conn".to_string())
                            .spawn(move || {
                                handle_k_request(stream, root.as_path(), api_host.as_str(), api_port);
                            });
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    Err(_) => break,
                }
            })
            .map_err(|err| format!("K-Social server spawn failed: {err}"))?;

        Ok(Self {
            host,
            port,
            api_host,
            api_port,
            stop_tx,
            join: Some(join),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for KSocialServer {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn find_k_build_root() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("K").join("dist"));
        for ancestor in cwd.ancestors().skip(1).take(4) {
            candidates.push(ancestor.join("K").join("dist"));
        }
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("K").join("dist"));
        for ancestor in dir.ancestors().skip(1).take(4) {
            candidates.push(ancestor.join("K").join("dist"));
        }
    }

    candidates
        .into_iter()
        .find(|path| path.join("index.html").exists())
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_k_request(mut stream: TcpStream, root: &Path, api_host: &str, api_port: u16) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

    let request = match read_http_request(&mut stream) {
        Some(request) => request,
        None => return,
    };
    if request.is_empty() {
        return;
    }

    let request_text = String::from_utf8_lossy(&request);
    let mut method = "";
    let mut path = "/";
    if let Some(line) = request_text.lines().next() {
        let mut parts = line.split_whitespace();
        method = parts.next().unwrap_or_default();
        path = parts.next().unwrap_or("/");
    }
    let http_version = request_text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap_or("HTTP/1.1");
    let connection_header = request_text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("connection") {
            Some(value.trim().to_ascii_lowercase())
        } else {
            None
        }
    });
    let keep_alive = if http_version.eq_ignore_ascii_case("HTTP/1.0") {
        connection_header.as_deref() == Some("keep-alive")
    } else {
        connection_header.as_deref() != Some("close")
    };
    let range_header = request_text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("range") {
            Some(value.trim().to_string())
        } else {
            None
        }
    });

    let normalized = path.split('?').next().unwrap_or("/");

    // Disable service-worker behavior for embedded/browser debugging.
    // Stale SW caches can keep old JS/WASM pairs and cause load mismatches.
    if normalized == "/registerSW.js" {
        let body = b"/* kaspa-ng: service worker disabled */";
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store, no-cache, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nConnection: {}\r\n{}\r\n",
            body.len(),
            if keep_alive { "keep-alive" } else { "close" },
            if keep_alive {
                "Keep-Alive: timeout=5, max=100"
            } else {
                ""
            }
        );
        let _ = write_all_with_retry(&mut stream, headers.as_bytes());
        let _ = write_all_with_retry(&mut stream, body);
        let _ = stream.flush();
        log_info!("k-social-server: {} {} -> 200 OK (sw disabled)", method, normalized);
        return;
    }
    if normalized == "/sw.js" || normalized.starts_with("/workbox-") {
        let body = b"// kaspa-ng: service worker disabled";
        let headers = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/javascript; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store, no-cache, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nConnection: {}\r\n{}\r\n",
            body.len(),
            if keep_alive { "keep-alive" } else { "close" },
            if keep_alive {
                "Keep-Alive: timeout=5, max=100"
            } else {
                ""
            }
        );
        let _ = write_all_with_retry(&mut stream, headers.as_bytes());
        let _ = write_all_with_retry(&mut stream, body);
        let _ = stream.flush();
        log_info!("k-social-server: {} {} -> 404 Not Found (sw disabled)", method, normalized);
        return;
    }

    if normalized == "/api" || normalized.starts_with("/api/") {
        if proxy_k_api_request(&mut stream, &request, api_host, api_port, normalized) {
            log_info!(
                "k-social-server: {} {} -> proxied to {}:{}",
                method,
                normalized,
                api_host,
                api_port
            );
            return;
        }

        let body = b"{\"error\":\"K-indexer API unavailable\"}";
        let headers = format!(
            "HTTP/1.1 502 Bad Gateway\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: {}\r\n{}\r\n",
            body.len(),
            if keep_alive { "keep-alive" } else { "close" },
            if keep_alive {
                "Keep-Alive: timeout=5, max=100"
            } else {
                ""
            }
        );
        let _ = write_all_with_retry(&mut stream, headers.as_bytes());
        let _ = write_all_with_retry(&mut stream, body);
        let _ = stream.flush();
        log_warn!(
            "k-social-server: {} {} -> proxy failed for {}:{}",
            method,
            normalized,
            api_host,
            api_port
        );
        return;
    }

    let relative = normalized.trim_start_matches('/');
    let mut candidate = if relative.is_empty() {
        root.join("index.html")
    } else {
        root.join(relative)
    };

    // Some K routes reference root assets with relative URLs (e.g. /user/Kaspa-logo.svg).
    // If the nested path does not exist, try resolving by filename from dist root.
    if !candidate.exists()
        && let Some(file_name) = std::path::Path::new(relative).file_name()
    {
        let root_asset_candidate = root.join(file_name);
        if root_asset_candidate.exists() && root_asset_candidate.is_file() {
            candidate = root_asset_candidate;
        }
    }

    let mut status = "200 OK";
    let body_path = if candidate.exists() && candidate.is_file() {
        candidate
    } else {
        status = "404 Not Found";
        root.join("index.html")
    };

    let content_type = match body_path.extension().and_then(|value| value.to_str()) {
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
    };

    match std::fs::File::open(&body_path) {
        Ok(mut file) => {
            let file_len = file.metadata().map(|meta| meta.len()).unwrap_or_default();
            let mut response_status = status.to_string();
            let (mut send_from, mut send_to) = (0_u64, file_len.saturating_sub(1));

            if let Some(range) = range_header.as_deref()
                && let Some(value) = range.strip_prefix("bytes=")
            {
                let mut parts = value.splitn(2, '-');
                let start = parts.next().and_then(|v| v.trim().parse::<u64>().ok());
                let end = parts.next().and_then(|v| {
                    if v.trim().is_empty() {
                        None
                    } else {
                        v.trim().parse::<u64>().ok()
                    }
                });
                if let Some(start) = start {
                    if start < file_len {
                        send_from = start;
                        send_to = end.unwrap_or(file_len.saturating_sub(1)).min(file_len.saturating_sub(1));
                        response_status = "206 Partial Content".to_string();
                    }
                }
            }

            let content_len = send_to.saturating_sub(send_from).saturating_add(1);
            let mut headers = format!(
                "HTTP/1.1 {response_status}\r\nContent-Type: {content_type}\r\nContent-Length: {content_len}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store, no-cache, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nConnection: {}\r\n",
                if keep_alive { "keep-alive" } else { "close" }
            );
            if keep_alive {
                headers.push_str("Keep-Alive: timeout=5, max=100\r\n");
            }
            if response_status.starts_with("206") {
                headers.push_str(&format!(
                    "Content-Range: bytes {send_from}-{send_to}/{file_len}\r\n"
                ));
            }
            headers.push_str("\r\n");

            if file.seek(SeekFrom::Start(send_from)).is_err()
                || write_all_with_retry(&mut stream, headers.as_bytes()).is_err()
            {
                log_warn!(
                    "k-social-server: {} {} -> header/seek failed (status {}, {})",
                    method,
                    normalized,
                    response_status,
                    content_type
                );
                return;
            }

            let mut limited = file.take(content_len);
            let copied_result = copy_to_stream_with_retry(&mut limited, &mut stream, content_len);
            let copied = copied_result.as_ref().ok().copied().unwrap_or_default();
            if copied != content_len || stream.flush().is_err() {
                if let Err(err) = copied_result {
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::BrokenPipe
                            | std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::ConnectionAborted
                    ) {
                        log_info!(
                            "k-social-server: {} {} -> client disconnected ({}, status {}, {})",
                            method,
                            normalized,
                            err,
                            response_status,
                            content_type
                        );
                        return;
                    }
                    log_warn!(
                        "k-social-server: {} {} -> body write error ({}, expected {}, wrote {}, status {}, {})",
                        method,
                        normalized,
                        err,
                        content_len,
                        copied,
                        response_status,
                        content_type
                    );
                    return;
                }
                if copied == 0 {
                    log_info!(
                        "k-social-server: {} {} -> client disconnected early (expected {}, wrote {}, status {}, {})",
                        method,
                        normalized,
                        content_len,
                        copied,
                        response_status,
                        content_type
                    );
                } else {
                    log_warn!(
                        "k-social-server: {} {} -> body write failed (expected {}, wrote {}, status {}, {})",
                        method,
                        normalized,
                        content_len,
                        copied,
                        response_status,
                        content_type
                    );
                }
                return;
            }

            log_info!(
                "k-social-server: {} {} -> {} ({}, bytes {}..{}/{})",
                method,
                normalized,
                response_status,
                content_type,
                send_from,
                send_to,
                file_len
            );
        }
        Err(err) => {
            let body = b"internal error";
            let headers = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: {}\r\n{}\r\n",
                body.len(),
                if keep_alive { "keep-alive" } else { "close" },
                if keep_alive {
                    "Keep-Alive: timeout=5, max=100"
                } else {
                    ""
                }
            );
            let _ = write_all_with_retry(&mut stream, headers.as_bytes());
            let _ = write_all_with_retry(&mut stream, body);
            let _ = stream.flush();
            log_warn!(
                "k-social-server: {} {} -> 500 open failed ({})",
                method,
                normalized,
                err
            );
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_all_with_retry(stream: &mut TcpStream, mut bytes: &[u8]) -> std::io::Result<()> {
    let mut retries = 0_u8;
    while !bytes.is_empty() {
        match stream.write(bytes) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "socket write returned zero",
                ));
            }
            Ok(written) => {
                bytes = &bytes[written..];
                retries = 0;
            }
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::Interrupted
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                ) && retries < 3 =>
            {
                retries += 1;
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn copy_to_stream_with_retry(
    reader: &mut dyn Read,
    stream: &mut TcpStream,
    expected: u64,
) -> std::io::Result<u64> {
    let mut written_total = 0_u64;
    let mut buf = [0_u8; 64 * 1024];
    while written_total < expected {
        let read = loop {
            match reader.read(&mut buf) {
                Ok(read) => break read,
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err),
            }
        };
        if read == 0 {
            break;
        }
        write_all_with_retry(stream, &buf[..read])?;
        written_total = written_total.saturating_add(read as u64);
    }
    Ok(written_total)
}

#[cfg(not(target_arch = "wasm32"))]
fn read_http_request(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut request = Vec::with_capacity(4096);
    let mut buf = [0_u8; 4096];
    let mut header_end: Option<usize> = None;
    let mut content_length = 0usize;

    loop {
        let bytes_read = stream.read(&mut buf).ok()?;
        if bytes_read == 0 {
            break;
        }

        request.extend_from_slice(&buf[..bytes_read]);
        if header_end.is_none() {
            if let Some(pos) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let end = pos + 4;
                header_end = Some(end);
                let header = String::from_utf8_lossy(&request[..end]);
                for line in header.lines() {
                    if let Some((name, value)) = line.split_once(':')
                        && name.trim().eq_ignore_ascii_case("content-length")
                    {
                        content_length = value.trim().parse::<usize>().unwrap_or(0);
                    }
                }
            }
        }

        if let Some(end) = header_end {
            let current_body_len = request.len().saturating_sub(end);
            if current_body_len >= content_length {
                break;
            }
        }
    }

    Some(request)
}

#[cfg(not(target_arch = "wasm32"))]
fn proxy_k_api_request(
    stream: &mut TcpStream,
    request: &[u8],
    api_host: &str,
    api_port: u16,
    normalized_path: &str,
) -> bool {
    let request_text = String::from_utf8_lossy(request);
    let header_end = match request.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(pos) => pos + 4,
        None => return false,
    };
    let body = &request[header_end..];

    let mut lines = request_text.lines();
    let first_line = match lines.next() {
        Some(line) => line,
        None => return false,
    };
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let path = parts.next().unwrap_or("/");
    let version = parts.next().unwrap_or("HTTP/1.1");

    let rewritten_path = if normalized_path == "/api" {
        "/"
    } else {
        path.strip_prefix("/api").unwrap_or(path)
    };

    let target = format!("{api_host}:{api_port}");
    let addrs = match target.to_socket_addrs() {
        Ok(addrs) => addrs.collect::<Vec<_>>(),
        Err(_) => return false,
    };
    if addrs.is_empty() {
        return false;
    }

    let mut upstream = match TcpStream::connect_timeout(&addrs[0], Duration::from_secs(2)) {
        Ok(stream) => stream,
        Err(_) => return false,
    };
    let _ = upstream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = upstream.set_write_timeout(Some(Duration::from_secs(10)));

    let mut forwarded = format!("{method} {rewritten_path} {version}\r\n");
    for line in request_text[..header_end.saturating_sub(4)].lines().skip(1) {
        if line.is_empty() {
            continue;
        }
        if let Some((name, _)) = line.split_once(':')
            && (name.trim().eq_ignore_ascii_case("host")
                || name.trim().eq_ignore_ascii_case("connection"))
        {
            continue;
        }
        forwarded.push_str(line);
        forwarded.push_str("\r\n");
    }
    forwarded.push_str(&format!("Host: {api_host}:{api_port}\r\n"));
    forwarded.push_str("Connection: close\r\n");
    forwarded.push_str("\r\n");

    if upstream.write_all(forwarded.as_bytes()).is_err() {
        return false;
    }
    if !body.is_empty() && upstream.write_all(body).is_err() {
        return false;
    }
    if upstream.flush().is_err() {
        return false;
    }

    let mut response = Vec::with_capacity(8192);
    let mut buf = [0_u8; 4096];
    loop {
        match upstream.read(&mut buf) {
            Ok(0) => break,
            Ok(read) => response.extend_from_slice(&buf[..read]),
            Err(_) => return false,
        }
    }

    stream.write_all(&response).is_ok() && stream.flush().is_ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn k_runtime_config_script(
    indexer_host: &str,
    indexer_port: u16,
    network: Network,
    kaspa_node_url: &str,
) -> String {
    let network = match network {
        Network::Mainnet => "mainnet",
        Network::Testnet10 => "testnet-10",
        Network::Testnet12 => "testnet-10",
    };
    let _api_url = format!("http://{indexer_host}:{indexer_port}");
    let local_proxy_api_url = format!("http://{K_HOST}:{DEFAULT_K_PORT}/api");

    format!(
        r#"
(() => {{
  try {{
    const key = "kaspa_user_settings";
    const current = localStorage.getItem(key);
    const parsed = current ? JSON.parse(current) : {{}};
    parsed.indexerType = "custom";
    parsed.customIndexerUrl = "{local_proxy_api_url}";
    parsed.kaspaConnectionType = "custom-node";
    parsed.customKaspaNodeUrl = "{kaspa_node_url}";
    parsed.selectedNetwork = "{network}";
    localStorage.setItem(key, JSON.stringify(parsed));
    window.__KASPA_NG_K_CONFIG = {{
      apiBaseUrl: "{local_proxy_api_url}",
      kaspaNodeUrl: "{kaspa_node_url}",
      network: "{network}"
    }};
  }} catch (_) {{}}
}})();
"# 
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn normalize_k_node_url(url: String) -> String {
    if url.starts_with("wrpcs://") {
        return url.replacen("wrpcs://", "wss://", 1);
    }
    if url.starts_with("wrpc://") {
        return url.replacen("wrpc://", "ws://", 1);
    }
    if url.starts_with("ws://") || url.starts_with("wss://") {
        return url;
    }
    format!("ws://{url}")
}

#[cfg(not(target_arch = "wasm32"))]
struct KIndexerHealth {
    ready: bool,
    status: String,
}

#[cfg(not(target_arch = "wasm32"))]
fn k_indexer_health(host: &str, port: u16) -> KIndexerHealth {
    let target = format!("{host}:{port}");
    if let Ok(addrs) = target.to_socket_addrs() {
        for addr in addrs {
            let mut stream = match TcpStream::connect_timeout(&addr, Duration::from_millis(500)) {
                Ok(stream) => stream,
                Err(_) => continue,
            };
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

            let request = format!(
                "GET /health HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n"
            );
            if stream.write_all(request.as_bytes()).is_err() {
                return KIndexerHealth {
                    ready: false,
                    status: "connected, request failed".to_string(),
                };
            }

            let mut response = [0_u8; 256];
            let bytes_read = stream.read(&mut response).unwrap_or_default();
            if bytes_read == 0 {
                return KIndexerHealth {
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
                Some(200..=299) => KIndexerHealth {
                    ready: true,
                    status: format!("ready ({line})"),
                },
                Some(code) => KIndexerHealth {
                    ready: false,
                    status: format!("not ready (HTTP {code})"),
                },
                None => KIndexerHealth {
                    ready: false,
                    status: "connected, invalid HTTP response".to_string(),
                },
            };
        }
    }
    KIndexerHealth {
        ready: false,
        status: "unreachable".to_string(),
    }
}
