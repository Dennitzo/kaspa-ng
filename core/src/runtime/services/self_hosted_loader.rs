use crate::imports::*;
use crate::runtime::services::{
    LogStore, LogStores, SelfHostedExplorerService, SelfHostedIndexerService,
    SelfHostedKIndexerService, SelfHostedKasiaIndexerService, SelfHostedPostgresService,
};
use tokio::net::TcpStream;
use tokio::time::{MissedTickBehavior, timeout};
use tokio_postgres::NoTls;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderStatusSnapshot {
    pub phase: String,
    pub message: String,
    pub connected: bool,
    pub postgres_ready: bool,
    pub indexers_ready: bool,
    pub rest_ready: bool,
    pub socket_ready: bool,
    pub last_ping_at: String,
}

impl Default for LoaderStatusSnapshot {
    fn default() -> Self {
        Self {
            phase: "Disabled".to_string(),
            message: "Loader is disabled".to_string(),
            connected: false,
            postgres_ready: false,
            indexers_ready: false,
            rest_ready: false,
            socket_ready: false,
            last_ping_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Clone, Default)]
pub struct SharedLoaderStatus {
    inner: Arc<Mutex<LoaderStatusSnapshot>>,
}

impl SharedLoaderStatus {
    pub fn snapshot(&self) -> LoaderStatusSnapshot {
        self.inner.lock().unwrap().clone()
    }

    pub fn update(&self, snapshot: LoaderStatusSnapshot) {
        *self.inner.lock().unwrap() = snapshot;
    }
}

pub enum SelfHostedLoaderEvents {
    Enable,
    Disable,
    UpdateSettings(SelfHostedSettings),
    UpdateNodeSettings(NodeSettings),
    Exit,
}

pub struct SelfHostedLoaderService {
    pub application_events: ApplicationEventsChannel,
    pub service_events: Channel<SelfHostedLoaderEvents>,
    pub task_ctl: Channel<()>,
    pub settings: Mutex<SelfHostedSettings>,
    pub node_settings: Mutex<NodeSettings>,
    pub is_enabled: AtomicBool,
    logs: Arc<LogStore>,
    status: SharedLoaderStatus,
    postgres_service: Arc<SelfHostedPostgresService>,
    indexer_service: Arc<SelfHostedIndexerService>,
    k_indexer_service: Arc<SelfHostedKIndexerService>,
    kasia_indexer_service: Arc<SelfHostedKasiaIndexerService>,
    explorer_service: Arc<SelfHostedExplorerService>,
    last_postgres_restart: Mutex<Option<Instant>>,
    last_indexer_restart: Mutex<Option<Instant>>,
    last_explorer_restart: Mutex<Option<Instant>>,
    network_switch_grace: Mutex<Option<(Instant, Network)>>,
    indexer_boot_started_at: Mutex<Option<Instant>>,
    explorer_boot_started_at: Mutex<Option<Instant>>,
    postgres_failures: Mutex<u32>,
    indexer_failures: Mutex<u32>,
    explorer_failures: Mutex<u32>,
    last_ping_log_at: Mutex<Option<Instant>>,
}

impl SelfHostedLoaderService {
    const TICK_INTERVAL: Duration = Duration::from_secs(2);
    const RESTART_COOLDOWN: Duration = Duration::from_secs(20);
    const PING_LOG_INTERVAL: Duration = Duration::from_secs(6);
    const POSTGRES_RESTART_FAILURE_THRESHOLD: u32 = 4;
    const INDEXER_RESTART_FAILURE_THRESHOLD: u32 = 3;
    const EXPLORER_RESTART_FAILURE_THRESHOLD: u32 = 3;
    const DEPENDENTS_STOP_GRACE: Duration = Duration::from_millis(1500);
    const INDEXER_BOOT_GRACE: Duration = Duration::from_secs(45);
    const EXPLORER_BOOT_GRACE: Duration = Duration::from_secs(25);
    const NETWORK_SWITCH_GRACE: Duration = Duration::from_secs(30);

    fn resolve_probe_host(bind: &str) -> String {
        let trimmed = bind.trim();
        if trimmed.is_empty() || trimmed == "0.0.0.0" || trimmed == "::" || trimmed == "[::]" {
            "127.0.0.1".to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn normalize_probe_host(host: &str) -> String {
        let trimmed = host.trim();
        if trimmed.is_empty() || trimmed == "0.0.0.0" || trimmed == "::" || trimmed == "[::]" {
            "127.0.0.1".to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn parse_host_port(input: &str) -> Option<(String, u16)> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        if let Ok(addr) = trimmed.parse::<std::net::SocketAddr>() {
            return Some((
                Self::normalize_probe_host(&addr.ip().to_string()),
                addr.port(),
            ));
        }

        if trimmed.starts_with('[') {
            if let Some(end) = trimmed.find(']') {
                let host = &trimmed[..=end];
                let port = trimmed[end + 1..]
                    .trim_start_matches(':')
                    .parse::<u16>()
                    .ok()?;
                return Some((Self::normalize_probe_host(host), port));
            }
            return None;
        }

        let (host, port_raw) = trimmed.rsplit_once(':')?;
        let port = port_raw.parse::<u16>().ok()?;
        Some((Self::normalize_probe_host(host), port))
    }

    async fn check_tcp(host: &str, port: u16) -> bool {
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        let addr = format!("{host}:{port}");
        matches!(
            timeout(Duration::from_millis(1500), TcpStream::connect(addr)).await,
            Ok(Ok(_))
        )
    }

    async fn check_postgres(settings: &SelfHostedSettings, node: &NodeSettings) -> bool {
        let conn_str = format!(
            "host={} port={} user={} password={} dbname=postgres connect_timeout=3",
            settings.db_host,
            settings.effective_db_port(node.network),
            settings.db_user,
            settings.db_password
        );

        match timeout(
            Duration::from_secs(4),
            tokio_postgres::connect(&conn_str, NoTls),
        )
        .await
        {
            Ok(Ok((client, connection))) => {
                spawn(async move {
                    let _ = connection.await;
                    Ok(())
                });
                client.simple_query("SELECT 1").await.is_ok()
            }
            _ => false,
        }
    }

    async fn check_node_synced(node: &NodeSettings) -> bool {
        let wallet = runtime().wallet();
        match timeout(Duration::from_secs(4), wallet.clone().get_status(None)).await {
            Ok(Ok(status)) => {
                let network_matches = status
                    .network_id
                    .map(Network::from)
                    .map(|network| {
                        network == node.network
                            || matches!(
                                (node.network, network),
                                // Compatibility mode: Testnet12 wallet/rpc status is reported as testnet-10.
                                (Network::Testnet12, Network::Testnet10)
                            )
                    })
                    .unwrap_or(true);
                status.is_connected && status.is_synced && network_matches
            }
            _ => false,
        }
    }

    async fn check_indexer(settings: &SelfHostedSettings, node: &NodeSettings) -> bool {
        let listen = settings.effective_indexer_listen(node.network);
        let Some((host, port)) = Self::parse_host_port(&listen) else {
            return false;
        };
        Self::check_tcp(host.as_str(), port).await
    }

    fn should_run_k_indexer(settings: &SelfHostedSettings, node: &NodeSettings) -> bool {
        settings.k_enabled && matches!(node.network, Network::Mainnet)
    }

    fn should_run_kasia_indexer(settings: &SelfHostedSettings, node: &NodeSettings) -> bool {
        settings.kasia_enabled && matches!(node.network, Network::Mainnet)
    }

    fn should_restart(slot: &Mutex<Option<Instant>>) -> bool {
        let mut guard = slot.lock().unwrap();
        let allowed = guard
            .map(|last| last.elapsed() >= Self::RESTART_COOLDOWN)
            .unwrap_or(true);
        if allowed {
            *guard = Some(Instant::now());
        }
        allowed
    }

    fn health_failures(slot: &Mutex<u32>, healthy: bool) -> u32 {
        let mut guard = slot.lock().unwrap();
        if healthy {
            *guard = 0;
            0
        } else {
            *guard = guard.saturating_add(1);
            *guard
        }
    }

    fn reset_health_failures(&self) {
        *self.postgres_failures.lock().unwrap() = 0;
        *self.indexer_failures.lock().unwrap() = 0;
        *self.explorer_failures.lock().unwrap() = 0;
    }

    fn reset_restart_cooldowns(&self) {
        *self.last_postgres_restart.lock().unwrap() = None;
        *self.last_indexer_restart.lock().unwrap() = None;
        *self.last_explorer_restart.lock().unwrap() = None;
        *self.indexer_boot_started_at.lock().unwrap() = None;
        *self.explorer_boot_started_at.lock().unwrap() = None;
        self.reset_health_failures();
    }

    fn begin_network_switch_grace(&self, target: Network) {
        *self.network_switch_grace.lock().unwrap() =
            Some((Instant::now() + Self::NETWORK_SWITCH_GRACE, target));
    }

    fn clear_network_switch_grace(&self) {
        *self.network_switch_grace.lock().unwrap() = None;
    }

    fn network_switch_state(&self) -> Option<(Network, u64)> {
        let mut guard = self.network_switch_grace.lock().unwrap();
        if let Some((until, target)) = guard.as_ref() {
            let now = Instant::now();
            if now < *until {
                let remaining = until.saturating_duration_since(now).as_secs();
                return Some((target.clone(), remaining));
            }
        }
        *guard = None;
        None
    }

    fn publish_status(
        &self,
        phase: &str,
        message: String,
        connected: bool,
        postgres_ready: bool,
        indexers_ready: bool,
        rest_ready: bool,
        socket_ready: bool,
    ) {
        self.status.update(LoaderStatusSnapshot {
            phase: phase.to_string(),
            message,
            connected,
            postgres_ready,
            indexers_ready,
            rest_ready,
            socket_ready,
            last_ping_at: chrono::Utc::now().to_rfc3339(),
        });
        runtime().request_repaint();
    }

    fn publish_disabled(&self, message: &str) {
        self.publish_status(
            "Disabled",
            message.to_string(),
            false,
            false,
            false,
            false,
            false,
        );
    }

    fn maybe_log_ping(&self, line: String) {
        let mut guard = self.last_ping_log_at.lock().unwrap();
        let should_log = guard
            .map(|last| last.elapsed() >= Self::PING_LOG_INTERVAL)
            .unwrap_or(true);
        if !should_log {
            return;
        }
        *guard = Some(Instant::now());
        self.logs.push("INFO", &line);
    }

    async fn stop_dependents(&self) {
        self.explorer_service.enable(false);
        self.kasia_indexer_service.enable(false);
        self.k_indexer_service.enable(false);
        self.indexer_service.enable(false);
    }

    async fn stop_all(&self) {
        self.stop_dependents().await;
        self.postgres_service.enable(false);
    }

    async fn restart_postgres_stack(&self) {
        self.logs.push(
            "WARN",
            "postgres health check failed; restarting full self-hosted stack",
        );
        self.stop_dependents().await;
        sleep(Self::DEPENDENTS_STOP_GRACE).await;
        self.postgres_service.enable(false);
        sleep(Duration::from_millis(700)).await;
        self.postgres_service.enable(true);
    }

    async fn restart_indexers(&self, settings: &SelfHostedSettings, node: &NodeSettings) {
        self.logs.push(
            "WARN",
            "indexer health check failed; restarting indexer services",
        );
        self.explorer_service.enable(false);
        self.indexer_service.enable(false);
        self.k_indexer_service.enable(false);
        self.kasia_indexer_service.enable(false);
        sleep(Duration::from_millis(500)).await;
        self.indexer_service.enable(settings.indexer_enabled);
        self.k_indexer_service
            .enable(Self::should_run_k_indexer(settings, node));
        self.kasia_indexer_service
            .enable(Self::should_run_kasia_indexer(settings, node));
    }

    async fn restart_explorer(&self) {
        self.logs.push(
            "WARN",
            "REST/socket health check failed; restarting explorer services",
        );
        self.explorer_service.enable(false);
        sleep(Duration::from_millis(450)).await;
        self.explorer_service.enable(true);
    }

    async fn begin_network_switch(self: &Arc<Self>, previous: Network, next: Network) {
        self.logs.push(
            "INFO",
            &format!(
                "network switch detected: {} -> {}; entering loader grace window",
                previous.name(),
                next.name()
            ),
        );
        self.reset_restart_cooldowns();
        self.begin_network_switch_grace(next.clone());
        self.publish_status(
            "Switching network",
            format!("Switching network: {} -> {}", previous.name(), next.name()),
            false,
            false,
            false,
            false,
            false,
        );
        self.stop_dependents().await;
        sleep(Self::DEPENDENTS_STOP_GRACE).await;
        self.postgres_service.enable(false);
        sleep(Duration::from_millis(700)).await;
        self.postgres_service.enable(true);
    }

    async fn reconcile(self: &Arc<Self>) {
        let settings = self.settings.lock().unwrap().clone();
        let node = self.node_settings.lock().unwrap().clone();
        let network_switch_state = self.network_switch_state();
        let switching_network = network_switch_state.is_some();
        let switch_target = network_switch_state.as_ref().map(|(target, _)| target.clone());
        let switch_remaining = network_switch_state
            .as_ref()
            .map(|(_, remaining)| *remaining)
            .unwrap_or(0);

        if !self.is_enabled.load(Ordering::SeqCst) || !settings.enabled {
            self.stop_all().await;
            self.clear_network_switch_grace();
            self.publish_disabled("Loader is disabled");
            return;
        }

        if !settings.postgres_enabled {
            self.stop_dependents().await;
            self.postgres_service.enable(false);
            self.publish_status(
                "Initialisation",
                "Postgres is disabled; enable Postgres to continue".to_string(),
                false,
                false,
                false,
                false,
                false,
            );
            return;
        }

        let probe_host = Self::resolve_probe_host(&settings.api_bind);

        self.postgres_service.enable(true);
        let postgres_ready = Self::check_postgres(&settings, &node).await;
        let postgres_failures = if postgres_ready || switching_network {
            Self::health_failures(&self.postgres_failures, true)
        } else {
            Self::health_failures(&self.postgres_failures, false)
        };

        if !postgres_ready {
            self.stop_dependents().await;
            *self.indexer_boot_started_at.lock().unwrap() = None;
            *self.explorer_boot_started_at.lock().unwrap() = None;
            if !switching_network
                && postgres_failures >= Self::POSTGRES_RESTART_FAILURE_THRESHOLD
                && Self::should_restart(&self.last_postgres_restart)
            {
                self.restart_postgres_stack().await;
            }
            if let Some(target) = switch_target.as_ref() {
                self.publish_status(
                    "Switching network",
                    format!(
                        "Switching network to {} ({}s): waiting for Postgres",
                        target.name(),
                        switch_remaining
                    ),
                    false,
                    false,
                    false,
                    false,
                    false,
                );
            } else {
                self.publish_status(
                    "Initialisation",
                    "Waiting for Postgres".to_string(),
                    false,
                    false,
                    false,
                    false,
                    false,
                );
            }
            self.maybe_log_ping(
                "ping: postgres=down indexers=waiting rest=waiting socket=waiting".to_string(),
            );
            return;
        }

        let node_synced = Self::check_node_synced(&node).await;
        if !node_synced {
            self.stop_dependents().await;
            *self.indexer_boot_started_at.lock().unwrap() = None;
            *self.explorer_boot_started_at.lock().unwrap() = None;
            Self::health_failures(&self.indexer_failures, true);
            Self::health_failures(&self.explorer_failures, true);
            if let Some(target) = switch_target.as_ref() {
                self.publish_status(
                    "Switching network",
                    format!(
                        "Switching network to {} ({}s): waiting for Node sync",
                        target.name(),
                        switch_remaining
                    ),
                    false,
                    true,
                    false,
                    false,
                    false,
                );
            } else {
                self.publish_status(
                    "Initialisation",
                    "Waiting for Node sync".to_string(),
                    false,
                    true,
                    false,
                    false,
                    false,
                );
            }
            self.maybe_log_ping(
                "ping: postgres=ok node=syncing indexers=waiting rest=waiting socket=waiting"
                    .to_string(),
            );
            return;
        }

        self.indexer_service.enable(settings.indexer_enabled);
        self.k_indexer_service
            .enable(Self::should_run_k_indexer(&settings, &node));
        self.kasia_indexer_service
            .enable(Self::should_run_kasia_indexer(&settings, &node));

        let indexer_boot_started_at = {
            let mut guard = self.indexer_boot_started_at.lock().unwrap();
            if guard.is_none() {
                *guard = Some(Instant::now());
            }
            *guard
        };

        let indexer_ready = if settings.indexer_enabled {
            Self::check_indexer(&settings, &node).await
        } else {
            true
        };

        let k_indexer_required = Self::should_run_k_indexer(&settings, &node);
        let k_indexer_ready = if k_indexer_required {
            Self::check_tcp(&probe_host, settings.effective_k_web_port(node.network)).await
        } else {
            true
        };

        let kasia_required = Self::should_run_kasia_indexer(&settings, &node);
        let kasia_ready = if kasia_required {
            Self::check_tcp(
                &probe_host,
                settings.effective_kasia_indexer_port(node.network),
            )
            .await
        } else {
            true
        };

        let indexers_ready = indexer_ready && k_indexer_ready && kasia_ready;
        let indexer_in_grace = indexer_boot_started_at
            .map(|started| started.elapsed() < Self::INDEXER_BOOT_GRACE)
            .unwrap_or(false);
        let indexer_failures = if indexers_ready || indexer_in_grace || switching_network {
            Self::health_failures(&self.indexer_failures, true)
        } else {
            Self::health_failures(&self.indexer_failures, false)
        };
        if !indexers_ready {
            self.explorer_service.enable(false);
            *self.explorer_boot_started_at.lock().unwrap() = None;
            if !switching_network
                && !indexer_in_grace
                && indexer_failures >= Self::INDEXER_RESTART_FAILURE_THRESHOLD
                && Self::should_restart(&self.last_indexer_restart)
            {
                self.restart_indexers(&settings, &node).await;
            }

            let mut waiting = Vec::new();
            if settings.indexer_enabled && !indexer_ready {
                waiting.push("indexer");
            }
            if k_indexer_required && !k_indexer_ready {
                waiting.push("k-indexer");
            }
            if kasia_required && !kasia_ready {
                waiting.push("kasia-indexer");
            }
            let waiting = if waiting.is_empty() {
                "indexers".to_string()
            } else {
                waiting.join(", ")
            };

            if let Some(target) = switch_target.as_ref() {
                self.publish_status(
                    "Switching network",
                    format!(
                        "Switching network to {} ({}s): waiting for {waiting}",
                        target.name(),
                        switch_remaining
                    ),
                    false,
                    true,
                    false,
                    false,
                    false,
                );
            } else {
                self.publish_status(
                    "Initialisation",
                    format!("Waiting for {waiting}"),
                    false,
                    true,
                    false,
                    false,
                    false,
                );
            }
            self.maybe_log_ping(format!(
                "ping: postgres=ok indexers={} rest=waiting socket=waiting",
                if indexers_ready { "ok" } else { "down" }
            ));
            return;
        }

        *self.indexer_boot_started_at.lock().unwrap() = None;
        self.explorer_service.enable(true);
        let explorer_boot_started_at = {
            let mut guard = self.explorer_boot_started_at.lock().unwrap();
            if guard.is_none() {
                *guard = Some(Instant::now());
            }
            *guard
        };
        let rest_ready = Self::check_tcp(
            &probe_host,
            settings.effective_explorer_rest_port(node.network),
        )
        .await;
        let socket_ready = Self::check_tcp(
            &probe_host,
            settings.effective_explorer_socket_port(node.network),
        )
        .await;
        let explorer_ready = rest_ready && socket_ready;
        let explorer_in_grace = explorer_boot_started_at
            .map(|started| started.elapsed() < Self::EXPLORER_BOOT_GRACE)
            .unwrap_or(false);
        let explorer_failures = if explorer_ready || explorer_in_grace || switching_network {
            Self::health_failures(&self.explorer_failures, true)
        } else {
            Self::health_failures(&self.explorer_failures, false)
        };

        if !explorer_ready {
            if !switching_network
                && !explorer_in_grace
                && explorer_failures >= Self::EXPLORER_RESTART_FAILURE_THRESHOLD
                && Self::should_restart(&self.last_explorer_restart)
            {
                self.restart_explorer().await;
            }
            if let Some(target) = switch_target.as_ref() {
                self.publish_status(
                    "Switching network",
                    format!(
                        "Switching network to {} ({}s): waiting for REST API and socket server",
                        target.name(),
                        switch_remaining
                    ),
                    false,
                    true,
                    true,
                    rest_ready,
                    socket_ready,
                );
            } else {
                self.publish_status(
                    "Initialisation",
                    "Waiting for REST API and socket server".to_string(),
                    false,
                    true,
                    true,
                    rest_ready,
                    socket_ready,
                );
            }
            self.maybe_log_ping(format!(
                "ping: postgres=ok indexers=ok rest={} socket={}",
                if rest_ready { "ok" } else { "down" },
                if socket_ready { "ok" } else { "down" }
            ));
            return;
        }

        *self.explorer_boot_started_at.lock().unwrap() = None;
        if let Some(target) = switch_target {
            self.logs.push(
                "INFO",
                &format!("network switch to {} completed", target.name()),
            );
            self.clear_network_switch_grace();
        }
        self.publish_status(
            "Connected",
            "All self-hosted database services are running".to_string(),
            true,
            true,
            true,
            true,
            true,
        );
        self.maybe_log_ping("ping: postgres=ok indexers=ok rest=ok socket=ok".to_string());
    }

    pub fn new(
        application_events: ApplicationEventsChannel,
        settings: &Settings,
        logs: LogStores,
        status: SharedLoaderStatus,
        postgres_service: Arc<SelfHostedPostgresService>,
        indexer_service: Arc<SelfHostedIndexerService>,
        k_indexer_service: Arc<SelfHostedKIndexerService>,
        kasia_indexer_service: Arc<SelfHostedKasiaIndexerService>,
        explorer_service: Arc<SelfHostedExplorerService>,
    ) -> Self {
        Self {
            application_events,
            service_events: Channel::unbounded(),
            task_ctl: Channel::oneshot(),
            settings: Mutex::new(settings.self_hosted.clone()),
            node_settings: Mutex::new(settings.node.clone()),
            is_enabled: AtomicBool::new(false),
            logs: logs.loader,
            status,
            postgres_service,
            indexer_service,
            k_indexer_service,
            kasia_indexer_service,
            explorer_service,
            last_postgres_restart: Mutex::new(None),
            last_indexer_restart: Mutex::new(None),
            last_explorer_restart: Mutex::new(None),
            network_switch_grace: Mutex::new(None),
            indexer_boot_started_at: Mutex::new(None),
            explorer_boot_started_at: Mutex::new(None),
            postgres_failures: Mutex::new(0),
            indexer_failures: Mutex::new(0),
            explorer_failures: Mutex::new(0),
            last_ping_log_at: Mutex::new(None),
        }
    }

    pub fn enable(&self, enable: bool) {
        if enable {
            self.service_events
                .try_send(SelfHostedLoaderEvents::Enable)
                .unwrap();
        } else {
            self.service_events
                .try_send(SelfHostedLoaderEvents::Disable)
                .unwrap();
        }
    }

    pub fn update_settings(&self, settings: SelfHostedSettings) {
        self.service_events
            .try_send(SelfHostedLoaderEvents::UpdateSettings(settings))
            .unwrap();
    }

    pub fn update_node_settings(&self, settings: NodeSettings) {
        self.service_events
            .try_send(SelfHostedLoaderEvents::UpdateNodeSettings(settings))
            .unwrap();
    }
}

#[async_trait]
impl Service for SelfHostedLoaderService {
    fn name(&self) -> &'static str {
        "self-hosted-loader"
    }

    async fn spawn(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Self::TICK_INTERVAL);
            tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

            if this.is_enabled.load(Ordering::SeqCst) {
                this.logs.push(
                    "INFO",
                    "loader enabled: startup order postgres -> node sync -> indexers -> rest/socket",
                );
                this.reconcile().await;
            } else {
                this.publish_disabled("Loader is disabled");
            }

            loop {
                select! {
                    msg = this.service_events.receiver.recv().fuse() => {
                        match msg {
                            Ok(SelfHostedLoaderEvents::Enable) => {
                                let was_enabled = this.is_enabled.swap(true, Ordering::SeqCst);
                                if !was_enabled {
                                    this.logs.push("INFO", "loader enabled");
                                    this.clear_network_switch_grace();
                                    this.reset_restart_cooldowns();
                                }
                                this.reconcile().await;
                            }
                            Ok(SelfHostedLoaderEvents::Disable) => {
                                let was_enabled = this.is_enabled.swap(false, Ordering::SeqCst);
                                if was_enabled {
                                    this.logs.push("INFO", "loader disabled");
                                }
                                this.clear_network_switch_grace();
                                this.reset_restart_cooldowns();
                                this.stop_all().await;
                                this.publish_disabled("Loader is disabled");
                            }
                            Ok(SelfHostedLoaderEvents::UpdateSettings(settings)) => {
                                this.postgres_service.update_settings(settings.clone());
                                this.indexer_service.update_settings(settings.clone());
                                this.explorer_service.update_settings(settings.clone());
                                this.k_indexer_service.update_settings(settings.clone());
                                this.kasia_indexer_service.update_settings(settings.clone());
                                *this.settings.lock().unwrap() = settings;
                                this.reset_restart_cooldowns();
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    this.publish_status(
                                        "Initialisation",
                                        "Applying updated self-hosted settings".to_string(),
                                        false,
                                        false,
                                        false,
                                        false,
                                        false,
                                    );
                                    this.reconcile().await;
                                }
                            }
                            Ok(SelfHostedLoaderEvents::UpdateNodeSettings(settings)) => {
                                let previous_node = this.node_settings.lock().unwrap().clone();
                                this.postgres_service.update_node_settings(settings.clone());
                                this.indexer_service.update_node_settings(settings.clone());
                                this.explorer_service.update_node_settings(settings.clone());
                                this.k_indexer_service.update_node_settings(settings.clone());
                                this.kasia_indexer_service.update_node_settings(settings.clone());
                                *this.node_settings.lock().unwrap() = settings;
                                let next_node = this.node_settings.lock().unwrap().clone();
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    if previous_node.network != next_node.network {
                                        this.begin_network_switch(previous_node.network, next_node.network)
                                            .await;
                                    } else {
                                        this.reset_restart_cooldowns();
                                        this.publish_status(
                                            "Initialisation",
                                            "Applying updated network settings".to_string(),
                                            false,
                                            false,
                                            false,
                                            false,
                                            false,
                                        );
                                    }
                                    this.reconcile().await;
                                }
                            }
                            Ok(SelfHostedLoaderEvents::Exit) | Err(_) => {
                                this.is_enabled.store(false, Ordering::SeqCst);
                                this.stop_all().await;
                                this.publish_disabled("Loader stopped");
                                break;
                            }
                        }
                    }
                    _ = tick.tick().fuse() => {
                        if this.is_enabled.load(Ordering::SeqCst) {
                            this.reconcile().await;
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
            .try_send(SelfHostedLoaderEvents::Exit);
    }

    async fn join(self: Arc<Self>) -> Result<()> {
        self.task_ctl.recv().await.unwrap();
        Ok(())
    }
}
