use crate::imports::*;
use crate::runtime::services::{LogStore, LogStores};
use std::net::TcpListener;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

pub enum SelfHostedKIndexerEvents {
    Enable,
    Disable,
    UpdateSettings(SelfHostedSettings),
    UpdateNodeSettings(NodeSettings),
    Exit,
}

pub struct SelfHostedKIndexerService {
    pub application_events: ApplicationEventsChannel,
    pub service_events: Channel<SelfHostedKIndexerEvents>,
    pub task_ctl: Channel<()>,
    pub settings: Mutex<SelfHostedSettings>,
    pub node_settings: Mutex<NodeSettings>,
    pub is_enabled: AtomicBool,
    logs: Arc<LogStore>,
    processor_child: Mutex<Option<Child>>,
    webserver_child: Mutex<Option<Child>>,
    last_blocked_reason: Mutex<Option<String>>,
}

impl SelfHostedKIndexerService {
    fn child_is_running(
        child: &mut Option<Child>,
        process_name: &str,
        logs: &Arc<LogStore>,
    ) -> bool {
        let Some(proc) = child.as_mut() else {
            return false;
        };

        match proc.try_wait() {
            Ok(Some(status)) => {
                logs.push(
                    "WARN",
                    &format!("{process_name} exited with status: {status}"),
                );
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

    fn k_network(node: &NodeSettings) -> Option<&'static str> {
        match node.network {
            Network::Mainnet => Some("mainnet"),
            Network::Testnet10 | Network::Testnet12 => None,
        }
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
                    && settings.self_hosted.k_enabled
                    && matches!(settings.node.network, Network::Mainnet),
            ),
            logs: logs.k_indexer,
            processor_child: Mutex::new(None),
            webserver_child: Mutex::new(None),
            last_blocked_reason: Mutex::new(None),
        }
    }

    pub fn enable(&self, enable: bool) {
        if enable {
            self.service_events
                .try_send(SelfHostedKIndexerEvents::Enable)
                .unwrap();
        } else {
            self.service_events
                .try_send(SelfHostedKIndexerEvents::Disable)
                .unwrap();
        }
    }

    pub fn update_settings(&self, settings: SelfHostedSettings) {
        self.service_events
            .try_send(SelfHostedKIndexerEvents::UpdateSettings(settings))
            .unwrap();
    }

    pub fn update_node_settings(&self, settings: NodeSettings) {
        self.service_events
            .try_send(SelfHostedKIndexerEvents::UpdateNodeSettings(settings))
            .unwrap();
    }

    fn effective_db_name(settings: &SelfHostedSettings, node: &NodeSettings) -> String {
        crate::settings::self_hosted_db_name_for_network(settings.db_name.as_str(), node.network)
    }

    async fn wait_for_database(settings: &SelfHostedSettings, node: &NodeSettings) -> Result<()> {
        let db_name = Self::effective_db_name(settings, node);
        let mut last_error: Option<String> = None;
        for attempt in 0..20 {
            let conn_str = format!(
                "host={} port={} user={} password={} dbname={} connect_timeout=3",
                settings.db_host,
                settings.db_port,
                settings.db_user,
                settings.db_password,
                db_name.as_str()
            );
            match tokio_postgres::connect(&conn_str, tokio_postgres::NoTls).await {
                Ok((_client, connection)) => {
                    spawn(async move {
                        let _ = connection.await;
                        Ok(())
                    });
                    return Ok(());
                }
                Err(err) => last_error = Some(err.to_string()),
            }
            let sleep_secs = if attempt < 5 { 2 } else { 3 };
            tokio::time::sleep(std::time::Duration::from_secs(sleep_secs)).await;
        }

        Err(Error::Custom(format!(
            "database not ready after retries: {}",
            last_error.unwrap_or_else(|| "unknown error".to_string())
        )))
    }

    fn log_blocked_once(&self, message: impl Into<String>) {
        let message = message.into();
        let mut last = self.last_blocked_reason.lock().unwrap();
        if last.as_ref() == Some(&message) {
            return;
        }
        *last = Some(message.clone());
        self.logs.push("WARN", &message);
        log_warn!("self-hosted-k-indexer: {message}");
    }

    fn clear_blocked_reason(&self) {
        self.last_blocked_reason.lock().unwrap().take();
    }

    fn find_binary(bin_name: &str) -> Option<PathBuf> {
        let bin = if cfg!(windows) {
            format!("{bin_name}.exe")
        } else {
            bin_name.to_string()
        };
        let rel_candidates = [
            format!("K-indexer/target/release/{bin}"),
            format!("target/release/{bin}"),
        ];

        for candidate in rel_candidates {
            let path = PathBuf::from(&candidate);
            if path.exists() {
                return Some(path);
            }
        }

        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            let path = dir.join(&bin);
            if path.exists() {
                return Some(path);
            }
            let path = dir
                .join("K-indexer")
                .join("target")
                .join("release")
                .join(&bin);
            if path.exists() {
                return Some(path);
            }
        }

        None
    }

    async fn start_processor(self: &Arc<Self>) -> Result<()> {
        {
            let mut guard = self.processor_child.lock().unwrap();
            if Self::child_is_running(&mut guard, "K-transaction-processor", &self.logs) {
                return Ok(());
            }
        }

        if self.processor_child.lock().unwrap().is_some() {
            return Ok(());
        }

        let settings = self.settings.lock().unwrap().clone();
        let node = self.node_settings.lock().unwrap().clone();
        if !settings.enabled || !settings.k_enabled {
            self.log_blocked_once(
                "K-indexer disabled by settings (self-hosted or k_enabled is false)",
            );
            return Ok(());
        }

        let Some(network) = Self::k_network(&node) else {
            self.log_blocked_once("K-indexer is only available on Mainnet");
            return Ok(());
        };

        if let Err(err) = Self::wait_for_database(&settings, &node).await {
            self.log_blocked_once(format!("waiting for database: {err}"));
            return Ok(());
        }
        let db_name = Self::effective_db_name(&settings, &node);

        let Some(binary) = Self::find_binary("K-transaction-processor") else {
            self.log_blocked_once("K-transaction-processor binary not found");
            return Ok(());
        };

        let mut cmd = Command::new(binary);
        cmd.arg("-H")
            .arg(&settings.db_host)
            .arg("-P")
            .arg(settings.db_port.to_string())
            .arg("-d")
            .arg(&db_name)
            .arg("-U")
            .arg(&settings.db_user)
            .arg("-p")
            .arg(&settings.db_password)
            .arg("-n")
            .arg(network);

        if settings.indexer_upgrade_db {
            cmd.arg("-u");
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
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
                self.log_blocked_once(format!("failed to start K-transaction-processor ({err})"));
                return Err(err);
            }
        };
        self.clear_blocked_reason();
        self.logs.push(
            "INFO",
            &format!(
                "started K-transaction-processor (network={network}, db={}:{}:{})",
                settings.db_host, settings.db_port, db_name
            ),
        );

        let logs_out = self.logs.clone();
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    logs_out.push("INFO", &format!("processor: {line}"));
                }
            });
        }

        let logs_err = self.logs.clone();
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    logs_err.push("WARN", &format!("processor: {line}"));
                }
            });
        }

        *self.processor_child.lock().unwrap() = Some(child);
        Ok(())
    }

    async fn start_webserver(self: &Arc<Self>) -> Result<()> {
        {
            let mut guard = self.webserver_child.lock().unwrap();
            if Self::child_is_running(&mut guard, "K-webserver", &self.logs) {
                return Ok(());
            }
        }

        if self.webserver_child.lock().unwrap().is_some() {
            return Ok(());
        }

        let settings = self.settings.lock().unwrap().clone();
        let node = self.node_settings.lock().unwrap().clone();
        if !settings.enabled || !settings.k_enabled {
            self.log_blocked_once(
                "K-webserver disabled by settings (self-hosted or k_enabled is false)",
            );
            return Ok(());
        }

        if Self::k_network(&node).is_none() {
            return Ok(());
        }

        let bind_host = Self::resolve_bind_host(&settings.api_bind);
        let listen = format!("{}:{}", bind_host, settings.k_web_port);
        if !Self::listen_addr_available(&listen) {
            self.log_blocked_once(format!(
                "K-webserver port already in use on {listen}; refusing to start"
            ));
            return Ok(());
        }

        if let Err(err) = Self::wait_for_database(&settings, &node).await {
            self.log_blocked_once(format!("waiting for database: {err}"));
            return Ok(());
        }
        let db_name = Self::effective_db_name(&settings, &node);

        let Some(binary) = Self::find_binary("K-webserver") else {
            self.log_blocked_once("K-webserver binary not found");
            return Ok(());
        };

        let mut cmd = Command::new(binary);
        cmd.arg("-H")
            .arg(&settings.db_host)
            .arg("-P")
            .arg(settings.db_port.to_string())
            .arg("-d")
            .arg(&db_name)
            .arg("-u")
            .arg(&settings.db_user)
            .arg("-p")
            .arg(&settings.db_password)
            .arg("-b")
            .arg(&listen);

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
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
                self.log_blocked_once(format!("failed to start K-webserver ({err})"));
                return Err(err);
            }
        };

        let logs_out = self.logs.clone();
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    logs_out.push("INFO", &format!("webserver: {line}"));
                }
            });
        }

        let logs_err = self.logs.clone();
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    logs_err.push("WARN", &format!("webserver: {line}"));
                }
            });
        }

        self.clear_blocked_reason();
        self.logs
            .push("INFO", &format!("K-webserver listening on {listen}"));
        *self.webserver_child.lock().unwrap() = Some(child);
        Ok(())
    }

    async fn stop_processor(&self) -> Result<()> {
        let child = self.processor_child.lock().unwrap().take();
        if let Some(mut child) = child {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        Ok(())
    }

    async fn stop_webserver(&self) -> Result<()> {
        let child = self.webserver_child.lock().unwrap().take();
        if let Some(mut child) = child {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        Ok(())
    }

    async fn start_all(self: &Arc<Self>) -> Result<()> {
        let settings = self.settings.lock().unwrap().clone();
        let node = self.node_settings.lock().unwrap().clone();
        let network_name = match node.network {
            Network::Mainnet => "mainnet",
            Network::Testnet10 => "testnet-10",
            Network::Testnet12 => "testnet-12",
        };
        self.logs.push(
            "INFO",
            &format!(
                "start requested (enabled={}, k_enabled={}, network={}, k_web_port={})",
                settings.enabled, settings.k_enabled, network_name, settings.k_web_port
            ),
        );
        log_info!(
            "self-hosted-k-indexer: start requested (enabled={}, k_enabled={}, network={}, k_web_port={})",
            settings.enabled,
            settings.k_enabled,
            network_name,
            settings.k_web_port
        );
        let _ = self.start_processor().await;
        let _ = self.start_webserver().await;
        Ok(())
    }

    async fn stop_all(&self) -> Result<()> {
        let _ = self.stop_webserver().await;
        let _ = self.stop_processor().await;
        Ok(())
    }
}

#[async_trait]
impl Service for SelfHostedKIndexerService {
    fn name(&self) -> &'static str {
        "self-hosted-k-indexer"
    }

    async fn spawn(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        tokio::spawn(async move {
            if this.is_enabled.load(Ordering::SeqCst) {
                let _ = this.start_all().await;
            } else {
                let settings = this.settings.lock().unwrap().clone();
                this.logs.push(
                    "INFO",
                    &format!(
                        "not starting: is_enabled=false (self-hosted={}, k_enabled={})",
                        settings.enabled, settings.k_enabled
                    ),
                );
                log_info!(
                    "self-hosted-k-indexer: not starting: is_enabled=false (self-hosted={}, k_enabled={})",
                    settings.enabled,
                    settings.k_enabled
                );
            }
            let mut retry_tick = tokio::time::interval(std::time::Duration::from_secs(5));

            loop {
                select! {
                    msg = this.service_events.receiver.recv().fuse() => {
                        match msg {
                            Ok(SelfHostedKIndexerEvents::Enable) => {
                                let was_enabled = this.is_enabled.swap(true, Ordering::SeqCst);
                                if !was_enabled {
                                    this.logs.push("INFO", "enable requested");
                                    log_info!("self-hosted-k-indexer: enable requested");
                                    let _ = this.start_all().await;
                                }
                            }
                            Ok(SelfHostedKIndexerEvents::Disable) => {
                                let was_enabled = this.is_enabled.swap(false, Ordering::SeqCst);
                                if was_enabled {
                                    this.logs.push("INFO", "disable requested");
                                    log_info!("self-hosted-k-indexer: disable requested");
                                    let _ = this.stop_all().await;
                                }
                            }
                            Ok(SelfHostedKIndexerEvents::UpdateSettings(settings)) => {
                                *this.settings.lock().unwrap() = settings;
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let _ = this.stop_all().await;
                                    let _ = this.start_all().await;
                                }
                            }
                            Ok(SelfHostedKIndexerEvents::UpdateNodeSettings(settings)) => {
                                *this.node_settings.lock().unwrap() = settings;
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let _ = this.stop_all().await;
                                    let _ = this.start_all().await;
                                }
                            }
                            Ok(SelfHostedKIndexerEvents::Exit) | Err(_) => {
                                let _ = this.stop_all().await;
                                break;
                            }
                        }
                    }
                    _ = retry_tick.tick().fuse() => {
                        if this.is_enabled.load(Ordering::SeqCst) {
                            let has_processor = {
                                let mut guard = this.processor_child.lock().unwrap();
                                Self::child_is_running(&mut guard, "K-transaction-processor", &this.logs)
                            };
                            let has_webserver = {
                                let mut guard = this.webserver_child.lock().unwrap();
                                Self::child_is_running(&mut guard, "K-webserver", &this.logs)
                            };
                            if !has_processor || !has_webserver {
                                this.logs.push(
                                    "INFO",
                                    "retrying K-indexer startup (some child processes are not running)",
                                );
                                log_info!(
                                    "self-hosted-k-indexer: retrying startup (processor_running={}, webserver_running={})",
                                    has_processor,
                                    has_webserver
                                );
                                let _ = this.start_all().await;
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
            .try_send(SelfHostedKIndexerEvents::Exit);
    }

    async fn join(self: Arc<Self>) -> Result<()> {
        self.task_ctl.recv().await.unwrap();
        Ok(())
    }
}
