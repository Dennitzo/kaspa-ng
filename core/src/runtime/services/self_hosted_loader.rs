use crate::imports::*;
use crate::runtime::services::{
    LogStore, LogStores, SelfHostedExplorerService, SelfHostedIndexerService,
    SelfHostedKIndexerService, SelfHostedKasiaIndexerService, SelfHostedPostgresService,
};
use std::path::{Path, PathBuf};
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

#[derive(Clone)]
pub struct SelfHostedLoaderServices {
    pub postgres_service: Arc<SelfHostedPostgresService>,
    pub indexer_service: Arc<SelfHostedIndexerService>,
    pub k_indexer_service: Arc<SelfHostedKIndexerService>,
    pub kasia_indexer_service: Arc<SelfHostedKasiaIndexerService>,
    pub explorer_service: Arc<SelfHostedExplorerService>,
}

#[derive(Clone, Copy)]
struct LoaderReadiness {
    connected: bool,
    postgres_ready: bool,
    indexers_ready: bool,
    rest_ready: bool,
    socket_ready: bool,
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
    postgres_boot_started_at: Mutex<Option<Instant>>,
    indexer_boot_started_at: Mutex<Option<Instant>>,
    explorer_boot_started_at: Mutex<Option<Instant>>,
    postgres_failures: Mutex<u32>,
    indexer_failures: Mutex<u32>,
    explorer_failures: Mutex<u32>,
    last_ping_log_at: Mutex<Option<Instant>>,
    last_postgres_debug_log_at: Mutex<Option<Instant>>,
}

impl SelfHostedLoaderService {
    const EXPECTED_POSTGRES_MAJOR: u32 = 15;
    const TICK_INTERVAL: Duration = Duration::from_secs(2);
    const RESTART_COOLDOWN: Duration = Duration::from_secs(20);
    const PING_LOG_INTERVAL: Duration = Duration::from_secs(6);
    const POSTGRES_RESTART_FAILURE_THRESHOLD: u32 = 4;
    const INDEXER_RESTART_FAILURE_THRESHOLD: u32 = 3;
    const EXPLORER_RESTART_FAILURE_THRESHOLD: u32 = 3;
    const DEPENDENTS_STOP_GRACE: Duration = Duration::from_millis(1500);
    const POSTGRES_BOOT_GRACE: Duration = Duration::from_secs(45);
    const INDEXER_BOOT_GRACE: Duration = Duration::from_secs(45);
    const EXPLORER_BOOT_GRACE: Duration = Duration::from_secs(25);
    const POSTGRES_DEBUG_LOG_INTERVAL: Duration = Duration::from_secs(12);

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

    fn postgres_candidate_bin_dirs() -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = Vec::new();

        if let Ok(exe) = std::env::current_exe()
            && let Some(exe_dir) = exe.parent()
        {
            dirs.push(exe_dir.join("postgres").join("bin"));

            #[cfg(target_os = "macos")]
            {
                dirs.push(exe_dir.join("../Resources/postgres/bin"));
                dirs.push(exe_dir.join("../../Resources/postgres/bin"));
            }
        }

        if let Ok(cwd) = std::env::current_dir() {
            dirs.push(cwd.join("postgres").join("bin"));
        }

        dirs
    }

    fn postgres_runtime_lib_dirs_for_binary(binary_path: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::<PathBuf>::new();
        let Some(bin_dir) = binary_path.parent() else {
            return dirs;
        };

        let candidates = [
            bin_dir.join("../lib"),
            bin_dir.join("../../lib"),
            bin_dir.join("../lib64"),
            bin_dir.join("../../lib64"),
        ];

        for dir in candidates {
            if let Ok(canonical) = std::fs::canonicalize(&dir)
                && canonical.is_dir()
                && !dirs.contains(&canonical)
            {
                dirs.push(canonical);
            }
        }

        dirs
    }

    fn merged_library_path(var_name: &str, extra_dirs: &[PathBuf]) -> Option<std::ffi::OsString> {
        if extra_dirs.is_empty() {
            return None;
        }
        let mut paths: Vec<PathBuf> = extra_dirs.to_vec();
        if let Some(existing) = std::env::var_os(var_name) {
            paths.extend(std::env::split_paths(&existing));
        }
        std::env::join_paths(paths).ok()
    }

    fn apply_postgres_runtime_env_for_binary(cmd: &mut std::process::Command, binary_path: &Path) {
        let extra_dirs = Self::postgres_runtime_lib_dirs_for_binary(binary_path);
        if extra_dirs.is_empty() {
            return;
        }

        #[cfg(target_os = "linux")]
        if let Some(value) = Self::merged_library_path("LD_LIBRARY_PATH", &extra_dirs) {
            cmd.env("LD_LIBRARY_PATH", value);
        }

        #[cfg(target_os = "macos")]
        if let Some(value) = Self::merged_library_path("DYLD_LIBRARY_PATH", &extra_dirs) {
            cmd.env("DYLD_LIBRARY_PATH", value);
        }
    }

    fn postgres_binary_major_version(postgres_path: &Path) -> Option<u32> {
        if !postgres_path.exists() || !postgres_path.is_file() {
            return None;
        }
        let mut cmd = std::process::Command::new(postgres_path);
        Self::apply_no_window_for_std_command(&mut cmd);
        Self::apply_postgres_runtime_env_for_binary(&mut cmd, postgres_path);
        let output = cmd
            .arg("--version")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8(output.stdout).ok()?;
        let token = text.split_whitespace().find(|part| {
            part.chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        })?;
        token
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok())
    }

    fn find_postgres_binary(binary: &str) -> Option<PathBuf> {
        let binary_name = if cfg!(windows) {
            format!("{binary}.exe")
        } else {
            binary.to_string()
        };

        for dir in Self::postgres_candidate_bin_dirs() {
            let candidate = dir.join(&binary_name);
            if !Self::runnable_binary(&candidate) {
                continue;
            }
            let postgres_candidate = dir.join(if cfg!(windows) {
                "postgres.exe"
            } else {
                "postgres"
            });
            let major_ok = Self::postgres_binary_major_version(&postgres_candidate)
                .map(|major| major == Self::EXPECTED_POSTGRES_MAJOR)
                .unwrap_or(false);
            if major_ok {
                return Some(candidate);
            }
        }

        None
    }

    fn runnable_binary(path: &Path) -> bool {
        if !path.exists() || !path.is_file() {
            return false;
        }
        let mut cmd = std::process::Command::new(path);
        Self::apply_no_window_for_std_command(&mut cmd);
        Self::apply_postgres_runtime_env_for_binary(&mut cmd, path);
        cmd.arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        matches!(cmd.status(), Ok(status) if status.success())
    }

    fn apply_no_window_for_std_command(_cmd: &mut std::process::Command) {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            _cmd.creation_flags(CREATE_NO_WINDOW);
        }
    }

    fn postgres_data_dir(settings: &SelfHostedSettings, node: &NodeSettings) -> Result<PathBuf> {
        if !settings.postgres_data_dir.trim().is_empty() {
            let mut path = PathBuf::from(settings.postgres_data_dir.trim());
            path.push(crate::settings::network_profile_slug(node.network));
            return Ok(path);
        }

        let default_storage_folder = kaspa_wallet_core::storage::local::default_storage_folder();
        let storage_folder = workflow_store::fs::resolve_path(default_storage_folder)?;
        Ok(storage_folder
            .join("self-hosted")
            .join("postgres")
            .join(crate::settings::network_profile_slug(node.network)))
    }

    fn maybe_log_postgres_install_debug(
        &self,
        settings: &SelfHostedSettings,
        node: &NodeSettings,
        context: &str,
    ) {
        let mut guard = self.last_postgres_debug_log_at.lock().unwrap();
        let should_log = guard
            .map(|last| last.elapsed() >= Self::POSTGRES_DEBUG_LOG_INTERVAL)
            .unwrap_or(true);
        if !should_log {
            return;
        }
        *guard = Some(Instant::now());

        let postgres_bin = Self::find_postgres_binary("postgres");
        let initdb_bin = Self::find_postgres_binary("initdb");
        let pg_ctl_bin = Self::find_postgres_binary("pg_ctl");
        let db_port = settings.effective_db_port(node.network);

        self.logs.push(
            "INFO",
            &format!(
                "postgres debug ({context}): host={} port={} user={} network={}",
                settings.db_host,
                db_port,
                settings.db_user,
                node.network.name()
            ),
        );

        match Self::postgres_data_dir(settings, node) {
            Ok(data_dir) => {
                let postmaster_pid = data_dir.join("postmaster.pid");
                self.logs.push(
                    "INFO",
                    &format!(
                        "postgres debug ({context}): data_dir={} exists={} postmaster.pid={}",
                        data_dir.display(),
                        data_dir.exists(),
                        if postmaster_pid.exists() {
                            "present"
                        } else {
                            "missing"
                        }
                    ),
                );
            }
            Err(err) => {
                self.logs.push(
                    "WARN",
                    &format!("postgres debug ({context}): failed to resolve data dir: {err}"),
                );
            }
        }

        self.logs.push(
            "INFO",
            &format!(
                "postgres debug ({context}): binaries postgres={} initdb={} pg_ctl={} expected_major={}",
                postgres_bin
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "not found".to_string()),
                initdb_bin
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "not found".to_string()),
                pg_ctl_bin
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "not found".to_string()),
                Self::EXPECTED_POSTGRES_MAJOR
            ),
        );

        let mut candidate_dirs = Self::postgres_candidate_bin_dirs();
        candidate_dirs.truncate(3);
        for dir in candidate_dirs {
            let postgres_path = dir.join(if cfg!(windows) {
                "postgres.exe"
            } else {
                "postgres"
            });
            let initdb_path = dir.join(if cfg!(windows) {
                "initdb.exe"
            } else {
                "initdb"
            });
            let pg_ctl_path = dir.join(if cfg!(windows) {
                "pg_ctl.exe"
            } else {
                "pg_ctl"
            });
            self.logs.push(
                "INFO",
                &format!(
                    "postgres debug ({context}): probe dir={} postgres(exists={},file={},major={}) initdb(exists={},file={}) pg_ctl(exists={},file={})",
                    dir.display(),
                    postgres_path.exists(),
                    postgres_path.is_file(),
                    Self::postgres_binary_major_version(&postgres_path)
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    initdb_path.exists(),
                    initdb_path.is_file(),
                    pg_ctl_path.exists(),
                    pg_ctl_path.is_file(),
                ),
            );
        }
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

    async fn check_tcp_any(host: &str, ports: &[u16]) -> bool {
        for port in ports {
            if Self::check_tcp(host, *port).await {
                return true;
            }
        }
        false
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
                    .map(|network| network == node.network)
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
        *self.postgres_boot_started_at.lock().unwrap() = None;
        *self.indexer_boot_started_at.lock().unwrap() = None;
        *self.explorer_boot_started_at.lock().unwrap() = None;
        self.reset_health_failures();
    }

    fn publish_status(&self, phase: &str, message: String, readiness: LoaderReadiness) {
        let next = LoaderStatusSnapshot {
            phase: phase.to_string(),
            message,
            connected: readiness.connected,
            postgres_ready: readiness.postgres_ready,
            indexers_ready: readiness.indexers_ready,
            rest_ready: readiness.rest_ready,
            socket_ready: readiness.socket_ready,
            last_ping_at: chrono::Utc::now().to_rfc3339(),
        };

        let current = self.status.snapshot();
        let unchanged = current.phase == next.phase
            && current.message == next.message
            && current.connected == next.connected
            && current.postgres_ready == next.postgres_ready
            && current.indexers_ready == next.indexers_ready
            && current.rest_ready == next.rest_ready
            && current.socket_ready == next.socket_ready;
        if unchanged {
            return;
        }

        self.status.update(next);
        runtime().request_repaint();
    }

    fn publish_disabled(&self, message: &str) {
        self.publish_status(
            "Disabled",
            message.to_string(),
            LoaderReadiness {
                connected: false,
                postgres_ready: false,
                indexers_ready: false,
                rest_ready: false,
                socket_ready: false,
            },
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

    async fn reconcile(self: &Arc<Self>) {
        let settings = self.settings.lock().unwrap().clone();
        let node = self.node_settings.lock().unwrap().clone();
        let switching_network = false;

        if !self.is_enabled.load(Ordering::SeqCst) || !settings.enabled {
            self.stop_all().await;
            self.publish_disabled("Loader is disabled");
            return;
        }

        if !settings.postgres_enabled {
            self.stop_dependents().await;
            self.postgres_service.enable(false);
            self.publish_status(
                "Initialisation",
                "Postgres is disabled; enable Postgres to continue".to_string(),
                LoaderReadiness {
                    connected: false,
                    postgres_ready: false,
                    indexers_ready: false,
                    rest_ready: false,
                    socket_ready: false,
                },
            );
            return;
        }

        let probe_host = Self::resolve_probe_host(&settings.api_bind);

        self.postgres_service.enable(true);
        let postgres_boot_started_at = {
            let mut guard = self.postgres_boot_started_at.lock().unwrap();
            if guard.is_none() {
                *guard = Some(Instant::now());
            }
            *guard
        };
        let postgres_ready = Self::check_postgres(&settings, &node).await;
        let postgres_in_grace = postgres_boot_started_at
            .map(|started| started.elapsed() < Self::POSTGRES_BOOT_GRACE)
            .unwrap_or(false);
        let postgres_failures = if postgres_ready || postgres_in_grace || switching_network {
            Self::health_failures(&self.postgres_failures, true)
        } else {
            Self::health_failures(&self.postgres_failures, false)
        };
        if postgres_ready {
            *self.postgres_boot_started_at.lock().unwrap() = None;
        }

        if !postgres_ready {
            self.maybe_log_postgres_install_debug(&settings, &node, "waiting for postgres");
            self.stop_dependents().await;
            *self.indexer_boot_started_at.lock().unwrap() = None;
            *self.explorer_boot_started_at.lock().unwrap() = None;
            if !switching_network
                && !postgres_in_grace
                && postgres_failures >= Self::POSTGRES_RESTART_FAILURE_THRESHOLD
                && Self::should_restart(&self.last_postgres_restart)
            {
                self.restart_postgres_stack().await;
            }
            self.publish_status(
                "Initialisation",
                "Waiting for Postgres".to_string(),
                LoaderReadiness {
                    connected: false,
                    postgres_ready: false,
                    indexers_ready: false,
                    rest_ready: false,
                    socket_ready: false,
                },
            );
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
            self.publish_status(
                "Initialisation",
                "Waiting for Node sync".to_string(),
                LoaderReadiness {
                    connected: false,
                    postgres_ready: true,
                    indexers_ready: false,
                    rest_ready: false,
                    socket_ready: false,
                },
            );
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
            let ports = SelfHostedKasiaIndexerService::health_probe_ports(&settings, &node);
            Self::check_tcp_any(&probe_host, &ports).await
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

            self.publish_status(
                "Initialisation",
                format!("Waiting for {waiting}"),
                LoaderReadiness {
                    connected: false,
                    postgres_ready: true,
                    indexers_ready: false,
                    rest_ready: false,
                    socket_ready: false,
                },
            );
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
            self.publish_status(
                "Initialisation",
                "Waiting for REST API and socket server".to_string(),
                LoaderReadiness {
                    connected: false,
                    postgres_ready: true,
                    indexers_ready: true,
                    rest_ready,
                    socket_ready,
                },
            );
            self.maybe_log_ping(format!(
                "ping: postgres=ok indexers=ok rest={} socket={}",
                if rest_ready { "ok" } else { "down" },
                if socket_ready { "ok" } else { "down" }
            ));
            return;
        }

        *self.explorer_boot_started_at.lock().unwrap() = None;
        self.publish_status(
            "Connected",
            "All self-hosted database services are running".to_string(),
            LoaderReadiness {
                connected: true,
                postgres_ready: true,
                indexers_ready: true,
                rest_ready: true,
                socket_ready: true,
            },
        );
        self.maybe_log_ping("ping: postgres=ok indexers=ok rest=ok socket=ok".to_string());
    }

    pub fn new(
        application_events: ApplicationEventsChannel,
        settings: &Settings,
        logs: LogStores,
        status: SharedLoaderStatus,
        services: SelfHostedLoaderServices,
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
            postgres_service: services.postgres_service,
            indexer_service: services.indexer_service,
            k_indexer_service: services.k_indexer_service,
            kasia_indexer_service: services.kasia_indexer_service,
            explorer_service: services.explorer_service,
            last_postgres_restart: Mutex::new(None),
            last_indexer_restart: Mutex::new(None),
            last_explorer_restart: Mutex::new(None),
            postgres_boot_started_at: Mutex::new(None),
            indexer_boot_started_at: Mutex::new(None),
            explorer_boot_started_at: Mutex::new(None),
            postgres_failures: Mutex::new(0),
            indexer_failures: Mutex::new(0),
            explorer_failures: Mutex::new(0),
            last_ping_log_at: Mutex::new(None),
            last_postgres_debug_log_at: Mutex::new(None),
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

    pub fn status_snapshot(&self) -> LoaderStatusSnapshot {
        self.status.snapshot()
    }

    pub fn log_snapshot(&self, limit: usize) -> Vec<crate::runtime::services::log_store::LogLine> {
        self.logs.snapshot(limit)
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
                                    this.reset_restart_cooldowns();
                                }
                                this.reconcile().await;
                            }
                            Ok(SelfHostedLoaderEvents::Disable) => {
                                let was_enabled = this.is_enabled.swap(false, Ordering::SeqCst);
                                if was_enabled {
                                    this.logs.push("INFO", "loader disabled");
                                }
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
                                        LoaderReadiness {
                                            connected: false,
                                            postgres_ready: false,
                                            indexers_ready: false,
                                            rest_ready: false,
                                            socket_ready: false,
                                        },
                                    );
                                    this.reconcile().await;
                                }
                            }
                            Ok(SelfHostedLoaderEvents::UpdateNodeSettings(settings)) => {
                                this.postgres_service.update_node_settings(settings.clone());
                                this.indexer_service.update_node_settings(settings.clone());
                                this.explorer_service.update_node_settings(settings.clone());
                                this.k_indexer_service.update_node_settings(settings.clone());
                                this.kasia_indexer_service.update_node_settings(settings.clone());
                                *this.node_settings.lock().unwrap() = settings;
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    this.reset_restart_cooldowns();
                                    this.publish_status(
                                        "Initialisation",
                                        "Applying updated network settings".to_string(),
                                        LoaderReadiness {
                                            connected: false,
                                            postgres_ready: false,
                                            indexers_ready: false,
                                            rest_ready: false,
                                            socket_ready: false,
                                        },
                                    );
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
