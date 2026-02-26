use crate::imports::*;
use crate::runtime::services::{LogStore, LogStores};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
#[cfg(unix)]
use tokio::time::timeout;

pub enum SelfHostedKasiaIndexerEvents {
    Enable,
    Disable,
    UpdateSettings(SelfHostedSettings),
    UpdateNodeSettings(NodeSettings),
    Exit,
}

pub struct SelfHostedKasiaIndexerService {
    pub application_events: ApplicationEventsChannel,
    pub service_events: Channel<SelfHostedKasiaIndexerEvents>,
    pub task_ctl: Channel<()>,
    pub settings: Mutex<SelfHostedSettings>,
    pub node_settings: Mutex<NodeSettings>,
    pub is_enabled: AtomicBool,
    logs: Arc<LogStore>,
    child: Mutex<Option<Child>>,
    last_blocked_reason: Mutex<Option<String>>,
}

impl SelfHostedKasiaIndexerService {
    #[cfg(unix)]
    async fn terminate_process_tree(child: &mut Child) {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;

        if let Some(pid) = child.id() {
            let pgid = Pid::from_raw(pid as i32);
            let _ = killpg(pgid, Signal::SIGTERM);
            if timeout(std::time::Duration::from_secs(2), child.wait())
                .await
                .is_ok()
            {
                return;
            }
            let _ = killpg(pgid, Signal::SIGKILL);
            let _ = timeout(std::time::Duration::from_secs(2), child.wait()).await;
        } else {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }

    fn child_is_running(child: &mut Option<Child>, process_name: &str, logs: &Arc<LogStore>) -> bool {
        let Some(proc) = child.as_mut() else {
            return false;
        };

        match proc.try_wait() {
            Ok(Some(status)) => {
                logs.push("WARN", &format!("{process_name} exited with status: {status}"));
                *child = None;
                false
            }
            Ok(None) => true,
            Err(err) => {
                logs.push("WARN", &format!("{process_name} state check failed: {err}"));
                *child = None;
                false
            }
        }
    }

    fn listen_addr_available(addr: &str) -> bool {
        TcpListener::bind(addr).is_ok()
    }

    fn resolve_bind_host(bind: &str) -> String {
        let trimmed = bind.trim();
        if trimmed.is_empty() || trimmed == "0.0.0.0" || trimmed == "::" || trimmed == "[::]" {
            "127.0.0.1".to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn kasia_network(node: &NodeSettings) -> Option<&'static str> {
        match node.network {
            Network::Mainnet => Some("mainnet"),
            Network::Testnet10 | Network::Testnet12 => None,
        }
    }

    fn sanitize_wrpc_host(value: &str) -> String {
        let mut host = value.trim().to_string();
        if let Some(rest) = host.strip_prefix("ws://") {
            host = rest.to_string();
        } else if let Some(rest) = host.strip_prefix("wss://") {
            host = rest.to_string();
        }
        if let Some((left, _)) = host.split_once('/') {
            host = left.to_string();
        }
        if host.starts_with('[') {
            if let Some(end) = host.find(']') {
                return host[..=end].to_string();
            }
            return host;
        }
        if host.matches(':').count() == 1
            && let Some((left, _)) = host.rsplit_once(':')
        {
            return left.to_string();
        }
        host
    }

    fn default_wrpc_port(network: Network) -> u16 {
        crate::settings::node_wrpc_borsh_port_for_network(network)
    }

    fn effective_wrpc_url(node: &NodeSettings) -> String {
        let host = {
            let sanitized = Self::sanitize_wrpc_host(&node.wrpc_url);
            if sanitized.trim().is_empty() {
                "127.0.0.1".to_string()
            } else {
                sanitized
            }
        };
        format!("ws://{}:{}", host, Self::default_wrpc_port(node.network))
    }

    fn default_db_root() -> PathBuf {
        workflow_core::dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".kasia-indexer")
    }

    fn find_binary() -> Option<PathBuf> {
        #[cfg(unix)]
        fn is_executable(path: &PathBuf) -> bool {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(path)
                .map(|meta| meta.is_file() && (meta.permissions().mode() & 0o111 != 0))
                .unwrap_or(false)
        }

        #[cfg(not(unix))]
        fn is_executable(path: &PathBuf) -> bool {
            std::fs::metadata(path).map(|meta| meta.is_file()).unwrap_or(false)
        }

        fn pick(path: PathBuf) -> Option<PathBuf> {
            if is_executable(&path) {
                Some(path)
            } else {
                None
            }
        }

        let app_bin = if cfg!(windows) {
            "kasia-indexer.exe"
        } else {
            "kasia-indexer"
        };
        let raw_bin = if cfg!(windows) { "indexer.exe" } else { "indexer" };
        let rel_candidates = [
            format!("target/release/{app_bin}"),
            format!("kasia-indexer/target/release/{raw_bin}"),
            format!("target/release/{raw_bin}"),
        ];

        for candidate in rel_candidates {
            let path = PathBuf::from(&candidate);
            if let Some(path) = pick(path) {
                return Some(path);
            }
        }

        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            let path = dir.join(app_bin);
            if let Some(path) = pick(path) {
                return Some(path);
            }

            // In macOS bundle builds, "kasia-indexer" may be a folder containing the binary.
            let nested_app_bin = dir.join("kasia-indexer").join(app_bin);
            if let Some(path) = pick(nested_app_bin) {
                return Some(path);
            }
            let nested_raw_bin = dir.join("kasia-indexer").join(raw_bin);
            if let Some(path) = pick(nested_raw_bin) {
                return Some(path);
            }

            let path = dir.join("kasia-indexer").join("target").join("release").join(raw_bin);
            if let Some(path) = pick(path) {
                return Some(path);
            }
        }

        None
    }

    fn log_blocked_once(&self, message: impl Into<String>) {
        let message = message.into();
        let mut last = self.last_blocked_reason.lock().unwrap();
        if last.as_ref() == Some(&message) {
            return;
        }
        *last = Some(message.clone());
        self.logs.push("WARN", &message);
        log_warn!("self-hosted-kasia-indexer: {message}");
    }

    fn clear_blocked_reason(&self) {
        self.last_blocked_reason.lock().unwrap().take();
    }

    pub fn new(
        application_events: ApplicationEventsChannel,
        settings: &Settings,
        logs: LogStores,
    ) -> Self {
        Self {
            application_events,
            service_events: Channel::unbounded(),
            task_ctl: Channel::oneshot(),
            settings: Mutex::new(settings.self_hosted.clone()),
            node_settings: Mutex::new(settings.node.clone()),
            is_enabled: AtomicBool::new(
                settings.self_hosted.enabled
                    && settings.self_hosted.kasia_enabled
                    && matches!(settings.node.network, Network::Mainnet),
            ),
            logs: logs.kasia_indexer,
            child: Mutex::new(None),
            last_blocked_reason: Mutex::new(None),
        }
    }

    pub fn enable(&self, enable: bool) {
        if enable {
            self.service_events
                .try_send(SelfHostedKasiaIndexerEvents::Enable)
                .unwrap();
        } else {
            self.service_events
                .try_send(SelfHostedKasiaIndexerEvents::Disable)
                .unwrap();
        }
    }

    pub fn update_settings(&self, settings: SelfHostedSettings) {
        self.service_events
            .try_send(SelfHostedKasiaIndexerEvents::UpdateSettings(settings))
            .unwrap();
    }

    pub fn update_node_settings(&self, settings: NodeSettings) {
        self.service_events
            .try_send(SelfHostedKasiaIndexerEvents::UpdateNodeSettings(settings))
            .unwrap();
    }

    async fn start_indexer(self: &Arc<Self>) -> Result<()> {
        {
            let mut guard = self.child.lock().unwrap();
            if Self::child_is_running(&mut guard, "kasia-indexer", &self.logs) {
                return Ok(());
            }
        }

        if self.child.lock().unwrap().is_some() {
            return Ok(());
        }

        let settings = self.settings.lock().unwrap().clone();
        let node = self.node_settings.lock().unwrap().clone();

        if !settings.enabled || !settings.kasia_enabled {
            self.log_blocked_once(
                "kasia-indexer disabled by settings (self-hosted or kasia_enabled is false)",
            );
            return Ok(());
        }

        let Some(network_type) = Self::kasia_network(&node) else {
            self.log_blocked_once("kasia-indexer is available only on Mainnet");
            return Ok(());
        };

        let bind_host = Self::resolve_bind_host(&settings.api_bind);
        let listen = format!("{}:{}", bind_host, settings.effective_kasia_indexer_port(node.network));
        if !Self::listen_addr_available(&listen) {
            self.log_blocked_once(format!(
                "kasia-indexer API port already in use on {listen}; refusing to start"
            ));
            return Ok(());
        }

        let Some(binary) = Self::find_binary() else {
            self.log_blocked_once("kasia-indexer binary not found");
            return Ok(());
        };

        self.logs.push(
            "INFO",
            &format!("starting kasia-indexer from {}", binary.display()),
        );

        let mut cmd = Command::new(&binary);
        let rust_backtrace = std::env::var("RUST_BACKTRACE").unwrap_or_else(|_| "1".to_string());
        let rust_lib_backtrace = std::env::var("RUST_LIB_BACKTRACE")
            .unwrap_or_else(|_| rust_backtrace.clone());
        cmd.env("NETWORK_TYPE", network_type)
            .env("KASPA_NODE_WBORSH_URL", Self::effective_wrpc_url(&node))
            .env("KASIA_INDEXER_DB_ROOT", Self::default_db_root())
            .env(
                "KASIA_INDEXER_API_BIND",
                format!("0.0.0.0:{}", settings.effective_kasia_indexer_port(node.network)),
            )
            .env("RUST_BACKTRACE", rust_backtrace)
            .env("RUST_LIB_BACKTRACE", rust_lib_backtrace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                let err = Error::NodeStartupError(err);
                self.log_blocked_once(format!(
                    "failed to start kasia-indexer from {} ({err})",
                    binary.display()
                ));
                return Err(err);
            }
        };

        self.clear_blocked_reason();
        self.logs.push(
            "INFO",
            &format!(
                "started kasia-indexer (network={network_type}, api=http://{listen})"
            ),
        );

        let logs_out = self.logs.clone();
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    logs_out.push("INFO", &line);
                }
            });
        }

        let logs_err = self.logs.clone();
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    logs_err.push("WARN", &line);
                }
            });
        }

        *self.child.lock().unwrap() = Some(child);
        Ok(())
    }

    async fn stop_indexer(&self) -> Result<()> {
        let child = self.child.lock().unwrap().take();
        if let Some(mut child) = child {
            #[cfg(unix)]
            {
                Self::terminate_process_tree(&mut child).await;
            }
            #[cfg(not(unix))]
            {
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Service for SelfHostedKasiaIndexerService {
    fn name(&self) -> &'static str {
        "self-hosted-kasia-indexer"
    }

    async fn spawn(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        tokio::spawn(async move {
            if this.is_enabled.load(Ordering::SeqCst) {
                let _ = this.start_indexer().await;
            }

            let mut retry_tick = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                select! {
                    msg = this.service_events.receiver.recv().fuse() => {
                        match msg {
                            Ok(SelfHostedKasiaIndexerEvents::Enable) => {
                                let was_enabled = this.is_enabled.swap(true, Ordering::SeqCst);
                                if !was_enabled {
                                    this.logs.push("INFO", "enable requested");
                                    let _ = this.start_indexer().await;
                                }
                            }
                            Ok(SelfHostedKasiaIndexerEvents::Disable) => {
                                let was_enabled = this.is_enabled.swap(false, Ordering::SeqCst);
                                if was_enabled {
                                    this.logs.push("INFO", "disable requested");
                                    let _ = this.stop_indexer().await;
                                }
                            }
                            Ok(SelfHostedKasiaIndexerEvents::UpdateSettings(settings)) => {
                                *this.settings.lock().unwrap() = settings;
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let _ = this.stop_indexer().await;
                                    let _ = this.start_indexer().await;
                                }
                            }
                            Ok(SelfHostedKasiaIndexerEvents::UpdateNodeSettings(settings)) => {
                                *this.node_settings.lock().unwrap() = settings;
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let _ = this.stop_indexer().await;
                                    let _ = this.start_indexer().await;
                                }
                            }
                            Ok(SelfHostedKasiaIndexerEvents::Exit) | Err(_) => {
                                let _ = this.stop_indexer().await;
                                break;
                            }
                        }
                    }
                    _ = retry_tick.tick().fuse() => {
                        if this.is_enabled.load(Ordering::SeqCst) {
                            let running = {
                                let mut guard = this.child.lock().unwrap();
                                Self::child_is_running(&mut guard, "kasia-indexer", &this.logs)
                            };
                            if !running {
                                this.logs.push("INFO", "retrying kasia-indexer startup");
                                let _ = this.start_indexer().await;
                            }
                        }
                    }
                }
            }

            this.task_ctl.send(()).await.unwrap();
        });

        Ok(())
    }

    fn terminate(self: Arc<Self>) {
        let _ = self
            .service_events
            .sender
            .try_send(SelfHostedKasiaIndexerEvents::Exit);
    }

    async fn join(self: Arc<Self>) -> Result<()> {
        self.task_ctl.recv().await.unwrap();
        Ok(())
    }
}
