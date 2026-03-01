use crate::imports::*;
use crate::runtime::services::{LogStore, LogStores};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
#[cfg(unix)]
use tokio::time::timeout;

pub enum SelfHostedIndexerEvents {
    Enable,
    Disable,
    UpdateSettings(SelfHostedSettings),
    UpdateNodeSettings(NodeSettings),
    Exit,
}

pub struct SelfHostedIndexerService {
    pub application_events: ApplicationEventsChannel,
    pub service_events: Channel<SelfHostedIndexerEvents>,
    pub task_ctl: Channel<()>,
    pub settings: Mutex<SelfHostedSettings>,
    pub node_settings: Mutex<NodeSettings>,
    pub is_enabled: AtomicBool,
    logs: Arc<LogStore>,
    child: Mutex<Option<Child>>,
}

impl SelfHostedIndexerService {
    fn pidfile_path(network: Network) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kaspa-ng-self-hosted-indexer-{}.pid",
            crate::settings::network_profile_slug(network)
        ))
    }

    fn write_pidfile(network: Network, pid: u32) {
        let _ = std::fs::write(Self::pidfile_path(network), pid.to_string());
    }

    fn remove_pidfile(network: Network) {
        let _ = std::fs::remove_file(Self::pidfile_path(network));
    }

    #[cfg(unix)]
    fn process_is_running(pid: u32) -> bool {
        use nix::errno::Errno;
        use nix::sys::signal::kill;
        use nix::unistd::Pid;

        let Ok(pid_i32) = i32::try_from(pid) else {
            return false;
        };

        match kill(Pid::from_raw(pid_i32), None) {
            Ok(_) => true,
            Err(Errno::EPERM) => true,
            Err(_) => false,
        }
    }

    #[cfg(unix)]
    async fn stop_pidfile_indexer_if_needed(
        listen: &str,
        node: &NodeSettings,
        logs: &Arc<LogStore>,
    ) {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        let path = Self::pidfile_path(node.network);
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return;
        };
        let Ok(pid) = raw.trim().parse::<u32>() else {
            let _ = std::fs::remove_file(&path);
            return;
        };
        if !Self::process_is_running(pid) {
            let _ = std::fs::remove_file(&path);
            return;
        }
        let Ok(pid_i32) = i32::try_from(pid) else {
            return;
        };

        logs.push(
            "WARN",
            &format!("indexer listen address busy on {listen}; stopping stale indexer pid {pid}"),
        );
        let _ = kill(Pid::from_raw(pid_i32), Signal::SIGTERM);

        for _ in 0..12 {
            if !Self::process_is_running(pid) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }

        if Self::process_is_running(pid) {
            let _ = kill(Pid::from_raw(pid_i32), Signal::SIGKILL);
            for _ in 0..8 {
                if !Self::process_is_running(pid) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
        let _ = std::fs::remove_file(&path);
    }

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

    fn listen_addr_available(addr: &str) -> bool {
        TcpListener::bind(addr).is_ok()
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
            is_enabled: AtomicBool::new(false),
            logs: logs.indexer,
            child: Mutex::new(None),
        }
    }

    pub fn enable(&self, enable: bool) {
        if enable {
            self.service_events
                .try_send(SelfHostedIndexerEvents::Enable)
                .unwrap();
        } else {
            self.service_events
                .try_send(SelfHostedIndexerEvents::Disable)
                .unwrap();
        }
    }

    pub fn update_settings(&self, settings: SelfHostedSettings) {
        self.service_events
            .try_send(SelfHostedIndexerEvents::UpdateSettings(settings))
            .unwrap();
    }

    pub fn update_node_settings(&self, settings: NodeSettings) {
        self.service_events
            .try_send(SelfHostedIndexerEvents::UpdateNodeSettings(settings))
            .unwrap();
    }

    fn effective_db_name(settings: &SelfHostedSettings, node: &NodeSettings) -> String {
        crate::settings::self_hosted_db_name_for_network(settings.db_name.as_str(), node.network)
    }

    fn build_database_url(settings: &SelfHostedSettings, node: &NodeSettings) -> String {
        let db_name = Self::effective_db_name(settings, node);
        let db_port = settings.effective_db_port(node.network);
        format!(
            "postgres://{}:{}@{}:{}/{}",
            settings.db_user, settings.db_password, settings.db_host, db_port, db_name
        )
    }

    fn default_wrpc_port(network: Network) -> u16 {
        crate::settings::node_wrpc_borsh_port_for_network(network)
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

    fn should_auto_adjust_indexer_rpc_url(url: &str) -> bool {
        let normalized = url.trim().to_ascii_lowercase();
        normalized.is_empty()
            || normalized.starts_with("ws://127.0.0.1:17110")
            || normalized.starts_with("ws://127.0.0.1:17210")
            || normalized.starts_with("ws://127.0.0.1:17310")
            || normalized.starts_with("ws://localhost:17110")
            || normalized.starts_with("ws://localhost:17210")
            || normalized.starts_with("ws://localhost:17310")
            || normalized.starts_with("ws://[::1]:17110")
            || normalized.starts_with("ws://[::1]:17210")
            || normalized.starts_with("ws://[::1]:17310")
    }

    fn effective_indexer_rpc_url(settings: &SelfHostedSettings, node: &NodeSettings) -> String {
        if !Self::should_auto_adjust_indexer_rpc_url(&settings.indexer_rpc_url) {
            return settings.indexer_rpc_url.clone();
        }

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

    fn indexer_network_arg(node: &NodeSettings) -> &'static str {
        match node.network {
            Network::Mainnet => "mainnet",
            Network::Testnet10 | Network::Testnet12 => "testnet-10",
        }
    }

    async fn wait_for_database(settings: &SelfHostedSettings, node: &NodeSettings) -> Result<()> {
        let db_name = Self::effective_db_name(settings, node);
        let db_port = settings.effective_db_port(node.network);
        let mut last_error: Option<String> = None;
        for attempt in 0..20 {
            let admin_conn_str = format!(
                "host={} port={} user={} password={} dbname=postgres connect_timeout=3",
                settings.db_host, db_port, settings.db_user, settings.db_password
            );
            match tokio_postgres::connect(&admin_conn_str, tokio_postgres::NoTls).await {
                Ok((admin_client, admin_connection)) => {
                    spawn(async move {
                        let _ = admin_connection.await;
                        Ok(())
                    });

                    let exists = admin_client
                        .query_opt("SELECT 1 FROM pg_database WHERE datname = $1", &[&db_name])
                        .await
                        .map(|row| row.is_some())
                        .unwrap_or(false);

                    if exists {
                        let conn_str = format!(
                            "host={} port={} user={} password={} dbname={} connect_timeout=3",
                            settings.db_host,
                            db_port,
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
                            Err(err) => {
                                last_error = Some(err.to_string());
                            }
                        }
                    } else {
                        last_error =
                            Some(format!("database '{}' is not ready yet", db_name.as_str()));
                    }
                }
                Err(err) => {
                    last_error = Some(err.to_string());
                }
            }
            let sleep_secs = if attempt < 5 { 2 } else { 3 };
            tokio::time::sleep(std::time::Duration::from_secs(sleep_secs)).await;
        }

        if let Some(err) = last_error {
            Err(Error::Custom(format!(
                "database not ready after retries: {err}"
            )))
        } else {
            Err(Error::Custom(
                "database not ready after retries".to_string(),
            ))
        }
    }

    fn find_indexer_binary(settings: &SelfHostedSettings) -> Option<PathBuf> {
        #[cfg(unix)]
        fn is_executable(path: &PathBuf) -> bool {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(path)
                .map(|meta| meta.is_file() && (meta.permissions().mode() & 0o111 != 0))
                .unwrap_or(false)
        }

        #[cfg(not(unix))]
        fn is_executable(path: &PathBuf) -> bool {
            std::fs::metadata(path)
                .map(|meta| meta.is_file())
                .unwrap_or(false)
        }

        #[cfg(unix)]
        fn ensure_executable(path: &PathBuf) {
            use std::os::unix::fs::PermissionsExt;
            if is_executable(path) {
                return;
            }
            if let Ok(meta) = std::fs::metadata(path) {
                let mut perms = meta.permissions();
                let mode = perms.mode() | 0o755;
                perms.set_mode(mode);
                let _ = std::fs::set_permissions(path, perms);
            }
        }

        #[cfg(not(unix))]
        fn ensure_executable(_path: &PathBuf) {}

        fn pick(path: PathBuf) -> Option<PathBuf> {
            ensure_executable(&path);
            if is_executable(&path) {
                Some(path)
            } else {
                None
            }
        }

        if !settings.indexer_binary.trim().is_empty() {
            let custom = PathBuf::from(settings.indexer_binary.trim());
            if let Some(custom) = pick(custom) {
                return Some(custom);
            }
        }

        let bin = if cfg!(windows) {
            "simply-kaspa-indexer.exe"
        } else {
            "simply-kaspa-indexer"
        };
        let rel_candidates = [
            bin.to_string(),
            format!("target/release/{bin}"),
            format!("target/debug/{bin}"),
            format!("simply-kaspa-indexer/target/release/{bin}"),
            format!("simply-kaspa-indexer/target/debug/{bin}"),
        ];

        for candidate in &rel_candidates {
            let path = PathBuf::from(candidate);
            if let Some(path) = pick(path) {
                return Some(path);
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for candidate in &rel_candidates {
                    let path = dir.join(candidate);
                    if let Some(path) = pick(path) {
                        return Some(path);
                    }
                }
            }
        }

        None
    }

    async fn start_indexer(self: &Arc<Self>) -> Result<()> {
        if self.child.lock().unwrap().is_some() {
            return Ok(());
        }

        let settings = self.settings.lock().unwrap().clone();
        let node = self.node_settings.lock().unwrap().clone();
        let indexer_listen = settings.effective_indexer_listen(node.network);
        if !settings.enabled || !settings.indexer_enabled {
            return Ok(());
        }

        if !Self::listen_addr_available(&indexer_listen) {
            #[cfg(unix)]
            {
                Self::stop_pidfile_indexer_if_needed(&indexer_listen, &node, &self.logs).await;
            }
        }

        if !Self::listen_addr_available(&indexer_listen) {
            log_warn!(
                "self-hosted-indexer: listen address already in use ({}); refusing to start indexer",
                indexer_listen
            );
            self.logs.push(
                "ERROR",
                &format!(
                    "listen address already in use ({}); refusing to start indexer",
                    indexer_listen
                ),
            );
            return Ok(());
        }

        if let Err(err) = Self::wait_for_database(&settings, &node).await {
            log_warn!("self-hosted-indexer: {err}");
            return Ok(());
        }

        let binary = match Self::find_indexer_binary(&settings) {
            Some(path) => path,
            None => {
                log_warn!("self-hosted-indexer: binary not found in default locations");
                self.logs.push(
                    "ERROR",
                    "binary not found; unable to start simply-kaspa-indexer",
                );
                return Ok(());
            }
        };

        let database_url = Self::build_database_url(&settings, &node);

        let mut cmd = Command::new(binary);
        let rpc_url = Self::effective_indexer_rpc_url(&settings, &node);
        let network_arg = Self::indexer_network_arg(&node);
        cmd.arg("-s")
            .arg(&rpc_url)
            .arg("-n")
            .arg(network_arg)
            .arg("-d")
            .arg(database_url)
            .arg("-l")
            .arg(&indexer_listen);

        if settings.indexer_upgrade_db {
            cmd.arg("-u");
        }

        let extra_args = settings.indexer_extra_args.trim();
        if !extra_args.is_empty() {
            let parts = extra_args
                .split_whitespace()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            cmd.args(parts);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
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
                log_warn!("self-hosted-indexer: failed to start ({err})");
                self.logs.push("ERROR", &format!("failed to start ({err})"));
                return Err(err);
            }
        };

        let logs_info = self.logs.clone();
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    log_info!("self-hosted-indexer: {line}");
                    logs_info.push("INFO", &line);
                }
            });
        }

        let logs_warn = self.logs.clone();
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    log_warn!("self-hosted-indexer: {line}");
                    logs_warn.push("WARN", &line);
                }
            });
        }

        if let Some(pid) = child.id() {
            Self::write_pidfile(node.network, pid);
        }
        *self.child.lock().unwrap() = Some(child);
        self.logs
            .push("INFO", &format!("using indexer rpc endpoint: {rpc_url}"));
        self.logs.push(
            "INFO",
            &format!("using indexer listen address: {indexer_listen}"),
        );
        let selected_network = node.network.to_string();
        if selected_network == "testnet-12" && network_arg == "testnet-10" {
            self.logs.push(
                "INFO",
                "using indexer network: testnet-10 (testnet-12 compatibility mode)",
            );
        } else {
            self.logs
                .push("INFO", &format!("using indexer network: {network_arg}"));
        }
        self.logs
            .push("INFO", &format!("selected app network: {selected_network}"));
        Ok(())
    }

    async fn stop_indexer(&self) -> Result<()> {
        let network = self.node_settings.lock().unwrap().network;
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
        Self::remove_pidfile(network);
        Ok(())
    }
}

#[async_trait]
impl Service for SelfHostedIndexerService {
    fn name(&self) -> &'static str {
        "self-hosted-indexer"
    }

    async fn spawn(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        tokio::spawn(async move {
            if this.is_enabled.load(Ordering::SeqCst) {
                let _ = this.start_indexer().await;
            }

            loop {
                select! {
                    msg = this.service_events.receiver.recv().fuse() => {
                        match msg {
                            Ok(SelfHostedIndexerEvents::Enable) => {
                                let was_enabled = this.is_enabled.swap(true, Ordering::SeqCst);
                                if !was_enabled {
                                    let _ = this.start_indexer().await;
                                }
                            }
                            Ok(SelfHostedIndexerEvents::Disable) => {
                                let was_enabled = this.is_enabled.swap(false, Ordering::SeqCst);
                                if was_enabled {
                                    let _ = this.stop_indexer().await;
                                }
                            }
                            Ok(SelfHostedIndexerEvents::UpdateSettings(settings)) => {
                                *this.settings.lock().unwrap() = settings;
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let _ = this.stop_indexer().await;
                                    let _ = this.start_indexer().await;
                                }
                            }
                            Ok(SelfHostedIndexerEvents::UpdateNodeSettings(settings)) => {
                                *this.node_settings.lock().unwrap() = settings;
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let _ = this.stop_indexer().await;
                                    let _ = this.start_indexer().await;
                                }
                            }
                            Ok(SelfHostedIndexerEvents::Exit) | Err(_) => {
                                let _ = this.stop_indexer().await;
                                break;
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
            .try_send(SelfHostedIndexerEvents::Exit);
    }

    async fn join(self: Arc<Self>) -> Result<()> {
        self.task_ctl.recv().await.unwrap();
        Ok(())
    }
}
