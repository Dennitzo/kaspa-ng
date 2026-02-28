use crate::imports::*;

#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpListener, TcpStream};
#[cfg(not(target_arch = "wasm32"))]
use std::process::Command;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::thread::JoinHandle;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
const KASVAULT_BIND_HOST: &str = "127.0.0.1";
#[cfg(not(target_arch = "wasm32"))]
const KASVAULT_PUBLIC_HOST: &str = "localhost";

pub struct Kasvault {
    #[allow(dead_code)]
    runtime: Runtime,
    #[cfg(not(target_arch = "wasm32"))]
    server: Option<KasVaultServer>,
    #[cfg(not(target_arch = "wasm32"))]
    status: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    open_requested: bool,
}

impl Kasvault {
    pub fn new(runtime: Runtime) -> Self {
        Self {
            runtime,
            #[cfg(not(target_arch = "wasm32"))]
            server: None,
            #[cfg(not(target_arch = "wasm32"))]
            status: None,
            #[cfg(not(target_arch = "wasm32"))]
            open_requested: false,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn reset_server(&mut self) {
        self.server = None;
        self.status = None;
        self.open_requested = false;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_local_server(&mut self, port: u16) {
        let port_changed = self
            .server
            .as_ref()
            .map(|server| server.port != port)
            .unwrap_or(false);
        if port_changed {
            self.server = None;
        }

        if self.server.is_some() {
            return;
        }

        match KasVaultServer::start(port) {
            Ok(server) => {
                self.status = None;
                self.server = Some(server);
            }
            Err(err) => {
                self.status = Some(err);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn open_in_browser(&mut self, browser: crate::settings::KasvaultBrowser, url: &str) {
        match open_url_with_browser(browser, url) {
            Ok(_) => {
                self.status = None;
            }
            Err(err) => {
                let message = format!("KasVault browser open failed: {err}");
                self.status = Some(message.clone());
                self.runtime.toast(UserNotification::error(message));
            }
        }
    }
}

impl ModuleT for Kasvault {
    fn name(&self) -> Option<&'static str> {
        Some(i18n("KasVault"))
    }

    fn activate(&mut self, core: &mut Core) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if !matches!(core.settings.node.network, Network::Mainnet)
                || !core.settings.kasvault.enabled
            {
                self.reset_server();
                return;
            }
            self.open_requested = true;
        }
    }

    fn network_change(&mut self, _core: &mut Core, _network: Network) {
        #[cfg(not(target_arch = "wasm32"))]
        self.reset_server();
    }

    fn render(
        &mut self,
        core: &mut Core,
        _ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        ui: &mut egui::Ui,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if !matches!(core.settings.node.network, Network::Mainnet) {
                self.reset_server();
                render_kasvault_centered_card(ui, |ui, compact| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(i18n("KasVault"))
                                .size(if compact { 24.0 } else { 28.0 })
                                .strong()
                                .color(theme_color().strong_color),
                        );
                        ui.add_space(if compact { 6.0 } else { 8.0 });
                        ui.colored_label(
                            theme_color().warning_color,
                            i18n("KasVault is available only on Mainnet."),
                        );
                    });
                });
                return;
            }

            if !core.settings.kasvault.enabled {
                self.reset_server();
                render_kasvault_centered_card(ui, |ui, compact| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(i18n("KasVault"))
                                .size(if compact { 24.0 } else { 28.0 })
                                .strong()
                                .color(theme_color().strong_color),
                        );
                        ui.add_space(if compact { 6.0 } else { 8.0 });
                        ui.label(i18n("Enable KasVault (Ledger) in Settings to use this tab."));
                    });
                });
                return;
            }

            let port = core
                .settings
                .user_interface
                .effective_kasvault_port(core.settings.node.network);
            self.ensure_local_server(port);

            if let Some(status) = &self.status {
                ui.colored_label(theme_color().warning_color, status);
            }

            let url = self.server.as_ref().map(|server| server.url.clone());
            let status = self.status.clone();

            if let Some(url) = &url
                && self.open_requested
            {
                self.open_requested = false;
                self.open_in_browser(core.settings.kasvault.browser, url);
            }

            render_kasvault_centered_card(ui, |ui, compact| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(i18n("KasVault"))
                            .size(if compact { 24.0 } else { 30.0 })
                            .strong()
                            .color(theme_color().strong_color),
                    );
                    ui.add_space(if compact { 4.0 } else { 6.0 });
                    ui.label(i18n(
                        "Secure Ledger workflow in your browser with local-only hosting.",
                    ));

                    ui.add_space(if compact { 10.0 } else { 14.0 });
                    let field_stroke =
                        Stroke::new(1.0, theme_color().kaspa_color.linear_multiply(0.40));

                    Frame::new()
                        .stroke(field_stroke)
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(if compact {
                            Margin::symmetric(10, 8)
                        } else {
                            Margin::symmetric(12, 10)
                        })
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(i18n("URL")).strong());
                                if let Some(url) = &url {
                                    ui.monospace(url.as_str());
                                } else {
                                    ui.colored_label(
                                        theme_color().warning_color,
                                        i18n("unavailable"),
                                    );
                                }
                            });
                        });

                    ui.add_space(if compact { 6.0 } else { 8.0 });
                    Frame::new()
                        .stroke(field_stroke)
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(if compact {
                            Margin::symmetric(10, 8)
                        } else {
                            Margin::symmetric(12, 10)
                        })
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(i18n("Browser")).strong());
                                ui.monospace(core.settings.kasvault.browser.label());
                            });
                        });

                    if let Some(status) = &status {
                        ui.add_space(if compact { 8.0 } else { 10.0 });
                        ui.colored_label(theme_color().warning_color, status);
                    }

                    ui.add_space(if compact { 10.0 } else { 12.0 });
                    ui.horizontal_centered(|ui| {
                        if ui
                            .add_enabled(url.is_some(), Button::new(i18n("Open in Browser")))
                            .clicked()
                            && let Some(url) = &url
                        {
                            self.open_in_browser(core.settings.kasvault.browser, url);
                        }
                    });
                });
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            let _ = core;
            ui.label(i18n("KasVault is not available in Web builds."));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn render_kasvault_centered_card<F>(ui: &mut Ui, add_contents: F)
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
                let fill = theme_color().kaspa_color.linear_multiply(0.08);
                let stroke = Stroke::new(1.0, theme_color().kaspa_color.linear_multiply(0.55));
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

#[cfg(not(target_arch = "wasm32"))]
fn open_url_with_browser(
    browser: crate::settings::KasvaultBrowser,
    url: &str,
) -> std::result::Result<(), String> {
    use crate::settings::KasvaultBrowser;

    if matches!(browser, KasvaultBrowser::SystemDefault) {
        return open::that(url).map_err(|err| err.to_string());
    }

    #[cfg(target_os = "macos")]
    {
        let app = match browser {
            KasvaultBrowser::Chrome => "Google Chrome",
            KasvaultBrowser::Firefox => "Firefox",
            KasvaultBrowser::Brave => "Brave Browser",
            KasvaultBrowser::Edge => "Microsoft Edge",
            KasvaultBrowser::Safari => "Safari",
            KasvaultBrowser::SystemDefault => unreachable!(),
        };
        return Command::new("open")
            .args(["-a", app, url])
            .spawn()
            .map(|_| ())
            .map_err(|err| err.to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let candidates: &[&str] = match browser {
            KasvaultBrowser::Chrome => &["chrome.exe", "chrome"],
            KasvaultBrowser::Firefox => &["firefox.exe", "firefox"],
            KasvaultBrowser::Brave => &["brave.exe", "brave", "brave-browser"],
            KasvaultBrowser::Edge => &["msedge.exe", "msedge"],
            KasvaultBrowser::Safari => &["safari.exe", "safari"],
            KasvaultBrowser::SystemDefault => unreachable!(),
        };
        return spawn_first_available(candidates, url);
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let candidates: &[&str] = match browser {
            KasvaultBrowser::Chrome => &["google-chrome", "google-chrome-stable", "chromium"],
            KasvaultBrowser::Firefox => &["firefox"],
            KasvaultBrowser::Brave => &["brave-browser", "brave"],
            KasvaultBrowser::Edge => &["microsoft-edge", "microsoft-edge-stable", "msedge"],
            KasvaultBrowser::Safari => &["safari"],
            KasvaultBrowser::SystemDefault => unreachable!(),
        };
        spawn_first_available(candidates, url)
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "macos")))]
fn spawn_first_available(candidates: &[&str], url: &str) -> std::result::Result<(), String> {
    let mut last_error: Option<String> = None;
    for candidate in candidates {
        match Command::new(candidate).arg(url).spawn() {
            Ok(_) => return Ok(()),
            Err(err) => last_error = Some(err.to_string()),
        }
    }

    Err(last_error.unwrap_or_else(|| "No browser command available".to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
struct KasVaultServer {
    url: String,
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl KasVaultServer {
    fn start(port: u16) -> std::result::Result<Self, String> {
        let root = find_kasvault_build_root().ok_or_else(|| {
            i18n("KasVault build not found. Run `npm install` and `npm run build` in `kasvault`.")
                .to_string()
        })?;

        let listener = TcpListener::bind((KASVAULT_BIND_HOST, port)).map_err(|err| {
            format!("KasVault server bind failed on {KASVAULT_BIND_HOST}:{port} ({err})")
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|err| format!("KasVault server nonblocking setup failed: {err}"))?;
        let addr = listener
            .local_addr()
            .map_err(|err| format!("KasVault server address error: {err}"))?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_signal = Arc::clone(&stop);
        let port = addr.port();
        let url = format!("http://{KASVAULT_PUBLIC_HOST}:{port}/");

        let thread = std::thread::Builder::new()
            .name("kasvault-server".to_string())
            .spawn(move || {
                while !stop_signal.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let root = root.clone();
                            std::thread::spawn(move || {
                                let _ = handle_kasvault_request(stream, root.as_path());
                            });
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(25));
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|err| format!("KasVault server spawn failed: {err}"))?;

        Ok(Self {
            url,
            port,
            stop,
            thread: Some(thread),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for KasVaultServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn find_kasvault_build_root() -> Option<PathBuf> {
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

    if !is_macos_bundle
        && let Ok(cwd) = std::env::current_dir()
    {
        candidates.push(cwd.join("kasvault").join("build"));
        candidates.push(cwd.join("KasVault").join("build"));
        for ancestor in cwd.ancestors().skip(1).take(4) {
            candidates.push(ancestor.join("kasvault").join("build"));
            candidates.push(ancestor.join("KasVault").join("build"));
        }
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("KasVault").join("build"));
        candidates.push(dir.join("kasvault").join("build"));
        for ancestor in dir.ancestors().skip(1).take(4) {
            candidates.push(ancestor.join("KasVault").join("build"));
            candidates.push(ancestor.join("kasvault").join("build"));
        }
    }

    candidates
        .into_iter()
        .find(|path| path.join("index.html").exists())
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_kasvault_request(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
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
            let headers = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store, no-cache, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(headers.as_bytes())?;
            stream.write_all(body)?;
            stream.flush()?;
            return Ok(());
        }
        file_path = root.join("index.html");
    }

    let body = std::fs::read(&file_path).unwrap_or_default();
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
