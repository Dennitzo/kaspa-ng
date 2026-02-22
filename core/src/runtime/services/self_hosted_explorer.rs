use crate::imports::*;
use crate::runtime::services::{LogStore, LogStores};
use std::net::TcpListener;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
#[cfg(unix)]
use tokio::time::{Duration, sleep, timeout};

const DEFAULT_GRPC_PORT: u16 = 16110;

pub enum SelfHostedExplorerEvents {
    Enable,
    Disable,
    UpdateSettings(SelfHostedSettings),
    UpdateNodeSettings(NodeSettings),
    Exit,
}

pub struct SelfHostedExplorerService {
    pub application_events: ApplicationEventsChannel,
    pub service_events: Channel<SelfHostedExplorerEvents>,
    pub task_ctl: Channel<()>,
    pub settings: Mutex<SelfHostedSettings>,
    pub node_settings: Mutex<NodeSettings>,
    pub is_enabled: AtomicBool,
    rest_logs: Arc<LogStore>,
    socket_logs: Arc<LogStore>,
    rest_child: Mutex<Option<Child>>,
    socket_child: Mutex<Option<Child>>,
}

impl SelfHostedExplorerService {
    fn detect_log_level<'a>(line: &str, fallback: &'a str) -> &'a str {
        let upper = line.to_ascii_uppercase();
        if upper.contains("CRITICAL") || upper.contains("[CRITICAL]") {
            "ERROR"
        } else if upper.contains("ERROR") || upper.contains("[ERROR]") {
            "ERROR"
        } else if upper.contains("WARN") || upper.contains("[WARN]") {
            "WARN"
        } else if upper.contains("INFO") || upper.contains("[INFO]") {
            "INFO"
        } else {
            fallback
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
            is_enabled: AtomicBool::new(settings.self_hosted.enabled),
            rest_logs: logs.rest,
            socket_logs: logs.socket,
            rest_child: Mutex::new(None),
            socket_child: Mutex::new(None),
        }
    }

    pub fn enable(&self, enable: bool) {
        if enable {
            self.service_events
                .try_send(SelfHostedExplorerEvents::Enable)
                .unwrap();
        } else {
            self.service_events
                .try_send(SelfHostedExplorerEvents::Disable)
                .unwrap();
        }
    }

    pub fn update_settings(&self, settings: SelfHostedSettings) {
        self.service_events
            .try_send(SelfHostedExplorerEvents::UpdateSettings(settings))
            .unwrap();
    }

    pub fn update_node_settings(&self, settings: NodeSettings) {
        self.service_events
            .try_send(SelfHostedExplorerEvents::UpdateNodeSettings(settings))
            .unwrap();
    }

    fn grpc_address_from_settings(settings: &NodeSettings) -> Option<String> {
        if !settings.enable_grpc {
            return None;
        }

        let addr = match settings.grpc_network_interface.kind {
            NetworkInterfaceKind::Local => "127.0.0.1".to_string(),
            NetworkInterfaceKind::Any => "127.0.0.1".to_string(),
            NetworkInterfaceKind::Custom => settings.grpc_network_interface.custom.to_string(),
        };

        if addr.contains(':') {
            Some(addr)
        } else {
            Some(format!("{addr}:{DEFAULT_GRPC_PORT}"))
        }
    }

    fn network_type(settings: &NodeSettings) -> &'static str {
        match settings.network {
            Network::Mainnet => "mainnet",
            Network::Testnet10 => "testnet",
            Network::Testnet12 => "testnet",
        }
    }

    fn build_sql_uri(settings: &SelfHostedSettings, node: &NodeSettings) -> String {
        let db_name = crate::settings::self_hosted_db_name_for_network(
            settings.db_name.as_str(),
            node.network,
        );
        let db_port = settings.effective_db_port(node.network);
        format!(
            "postgresql+asyncpg://{}:{}@{}:{}/{}",
            settings.db_user, settings.db_password, settings.db_host, db_port, db_name
        )
    }

    fn find_in_path(bin_name: &str) -> Option<PathBuf> {
        let path_var = std::env::var_os("PATH")?;
        std::env::split_paths(&path_var)
            .map(|dir| dir.join(bin_name))
            .find(|candidate| candidate.exists())
    }

    fn find_python() -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();

        #[cfg(target_os = "macos")]
        {
            candidates.extend(
                [
                    "/opt/homebrew/opt/python@3.12/bin/python3.12",
                    "/opt/homebrew/opt/python@3.11/bin/python3.11",
                    "/opt/homebrew/opt/python@3.10/bin/python3.10",
                    "/usr/local/opt/python@3.12/bin/python3.12",
                    "/usr/local/opt/python@3.11/bin/python3.11",
                    "/usr/local/opt/python@3.10/bin/python3.10",
                    "/opt/homebrew/opt/python/bin/python3",
                    "/usr/local/opt/python/bin/python3",
                ]
                .into_iter()
                .map(PathBuf::from),
            );
        }

        #[cfg(target_os = "linux")]
        {
            candidates.extend(
                [
                    "/usr/bin/python3.12",
                    "/usr/bin/python3.11",
                    "/usr/bin/python3.10",
                    "/usr/local/bin/python3.12",
                    "/usr/local/bin/python3.11",
                    "/usr/local/bin/python3.10",
                    "/usr/bin/python3",
                    "/usr/local/bin/python3",
                ]
                .into_iter()
                .map(PathBuf::from),
            );
        }

        #[cfg(target_os = "windows")]
        {
            candidates.extend(
                [
                    "C:\\Python312\\python.exe",
                    "C:\\Python311\\python.exe",
                    "C:\\Python310\\python.exe",
                ]
                .into_iter()
                .map(PathBuf::from),
            );
        }

        for candidate in candidates {
            if candidate.exists() {
                return Some(candidate);
            }
        }

        Self::find_in_path("python3").or_else(|| Self::find_in_path("python"))
    }

    fn find_poetry_compatible_python() -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();

        #[cfg(target_os = "macos")]
        {
            candidates.extend(
                [
                    "/opt/homebrew/opt/python@3.12/bin/python3.12",
                    "/opt/homebrew/opt/python@3.11/bin/python3.11",
                    "/opt/homebrew/opt/python@3.10/bin/python3.10",
                    "/usr/local/opt/python@3.12/bin/python3.12",
                    "/usr/local/opt/python@3.11/bin/python3.11",
                    "/usr/local/opt/python@3.10/bin/python3.10",
                ]
                .into_iter()
                .map(PathBuf::from),
            );
        }

        #[cfg(target_os = "linux")]
        {
            candidates.extend(
                ["/usr/bin/python3.12", "/usr/bin/python3.11", "/usr/bin/python3.10"]
                    .into_iter()
                    .map(PathBuf::from),
            );
        }

        for candidate in candidates {
            if candidate.exists() {
                return Some(candidate);
            }
        }

        Self::find_in_path("python3.12")
            .or_else(|| Self::find_in_path("python3.11"))
            .or_else(|| Self::find_in_path("python3.10"))
    }

    fn find_venv_python(root: &Path) -> Option<PathBuf> {
        let candidates = [
            root.join(".venv/bin/python3"),
            root.join(".venv/bin/python"),
            root.join(".venv/Scripts/python.exe"),
        ];
        candidates.into_iter().find(|path| path.exists())
    }

    fn find_server_root(name: &str) -> Option<PathBuf> {
        let mut candidates = Vec::new();

        if let Ok(root) = std::env::var("KASPA_NG_EXPLORER_SERVERS_ROOT") {
            candidates.push(PathBuf::from(root).join(name));
        }

        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join(name));
            for ancestor in cwd.ancestors().skip(1).take(4) {
                candidates.push(ancestor.join(name));
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join(name));
                for ancestor in dir.ancestors().skip(1).take(4) {
                    candidates.push(ancestor.join(name));
                }
            }
        }

        candidates
            .into_iter()
            .find(|dir| dir.join("main.py").exists())
    }

    fn build_command(root: &Path, bind: &str, port: u16) -> Option<Command> {
        let bind_arg = format!("{bind}:{port}");
        if let Some(venv_python) = Self::find_venv_python(root) {
            let mut cmd = Command::new(venv_python);
            cmd.arg("-m")
                .arg("gunicorn")
                .arg("-w")
                .arg("1")
                .arg("-k")
                .arg("uvicorn.workers.UvicornWorker")
                .arg("main:app")
                .arg("-b")
                .arg(&bind_arg);
            return Some(cmd);
        }
        if root.join("pyproject.toml").exists() {
            if let Some(poetry) = Self::find_in_path("poetry") {
                if let Some(py) = Self::find_poetry_compatible_python() {
                    let _ = std::process::Command::new(&poetry)
                        .current_dir(root)
                        .arg("env")
                        .arg("use")
                        .arg(py)
                        .status();
                }

                // Ensure runtime deps exist in the selected poetry env.
                if !Self::python_module_available(root, &poetry, &["run", "python"], "uvicorn") {
                    let _ = std::process::Command::new(&poetry)
                        .current_dir(root)
                        .arg("install")
                        .arg("--only")
                        .arg("main")
                        .arg("--no-root")
                        .arg("--no-interaction")
                        .status();
                }
                if Self::python_module_available(root, &poetry, &["run", "python"], "uvicorn") {
                    let mut cmd = Command::new(poetry);
                    cmd.arg("run")
                        .arg("python")
                        .arg("-m")
                        .arg("uvicorn")
                        .arg("main:app")
                        .arg("--host")
                        .arg(bind)
                        .arg("--port")
                        .arg(port.to_string());
                    return Some(cmd);
                }
            }
        }

        if root.join("Pipfile").exists() {
            if let Some(pipenv) = Self::find_in_path("pipenv") {
                let mut cmd = Command::new(pipenv);
                cmd.arg("run")
                    .arg("python")
                    .arg("-m")
                    .arg("uvicorn")
                    .arg("main:app")
                    .arg("--host")
                    .arg(bind)
                    .arg("--port")
                    .arg(port.to_string());
                return Some(cmd);
            }
        }

        let python = Self::find_python()?;
        let mut cmd = Command::new(python);
        cmd.arg("-m")
            .arg("uvicorn")
            .arg("main:app")
            .arg("--host")
            .arg(bind)
            .arg("--port")
            .arg(port.to_string());
        Some(cmd)
    }

    fn python_module_available(
        root: &Path,
        runner: &Path,
        runner_args: &[&str],
        module: &str,
    ) -> bool {
        let mut cmd = std::process::Command::new(runner);
        cmd.current_dir(root);
        for arg in runner_args {
            cmd.arg(arg);
        }
        cmd.arg("-c").arg(format!("import {module}"));
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        matches!(cmd.status(), Ok(status) if status.success())
    }

    fn apply_common_env(cmd: &mut Command, settings: &SelfHostedSettings, node: &NodeSettings) {
        cmd.env("SQL_URI", Self::build_sql_uri(settings, node));
        if let Some(grpc) = Self::grpc_address_from_settings(node) {
            cmd.env("KASPAD_HOST_1", grpc);
        }
        cmd.env("NETWORK_TYPE", Self::network_type(node));
        cmd.env("DEBUG", "false");
        cmd.env("PYTHONUNBUFFERED", "1");
    }

    fn port_is_available(bind: &str, port: u16) -> bool {
        let addr = format!("{bind}:{port}");
        TcpListener::bind(addr).is_ok()
    }

    async fn start_rest(self: &Arc<Self>) -> Result<()> {
        if self.rest_child.lock().unwrap().is_some() {
            return Ok(());
        }

        let settings = self.settings.lock().unwrap().clone();
        let node_settings = self.node_settings.lock().unwrap().clone();

        if !settings.enabled {
            return Ok(());
        }

        if Self::grpc_address_from_settings(&node_settings).is_none() {
            log_warn!("self-hosted-explorer: gRPC is disabled; REST server not started");
            return Ok(());
        }

        let rest_port = settings.effective_explorer_rest_port(node_settings.network);
        if !Self::port_is_available(&settings.api_bind, rest_port) {
            let msg = format!(
                "REST port already in use on {}:{}; refusing to start REST server",
                settings.api_bind, rest_port
            );
            log_warn!("self-hosted-explorer: {msg}");
            self.rest_logs.push("ERROR", &msg);
            return Ok(());
        }

        let root = match Self::find_server_root("kaspa-rest-server") {
            Some(root) => root,
            None => {
                log_warn!("self-hosted-explorer: kaspa-rest-server not found");
                return Ok(());
            }
        };

        let mut cmd = match Self::build_command(&root, &settings.api_bind, rest_port) {
            Some(cmd) => cmd,
            None => {
                log_warn!(
                    "self-hosted-explorer: python runtime not found; REST server not started"
                );
                return Ok(());
            }
        };

        cmd.current_dir(&root);
        Self::apply_common_env(&mut cmd, &settings, &node_settings);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                let err = Error::NodeStartupError(err);
                log_warn!("self-hosted-explorer: failed to start rest server ({err})");
                return Err(err);
            }
        };

        self.rest_logs.push(
            "INFO",
            &format!("REST API listening on {}:{}", settings.api_bind, rest_port),
        );

        let logs_info = self.rest_logs.clone();
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let level = Self::detect_log_level(&line, "INFO");
                    match level {
                        "ERROR" => log_warn!("self-hosted-rest: {line}"),
                        "WARN" => log_warn!("self-hosted-rest: {line}"),
                        _ => log_info!("self-hosted-rest: {line}"),
                    }
                    logs_info.push(level, &line);
                }
            });
        }

        let logs_warn = self.rest_logs.clone();
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let level = Self::detect_log_level(&line, "WARN");
                    match level {
                        "ERROR" => log_warn!("self-hosted-rest: {line}"),
                        "WARN" => log_warn!("self-hosted-rest: {line}"),
                        _ => log_info!("self-hosted-rest: {line}"),
                    }
                    logs_warn.push(level, &line);
                }
            });
        }

        *self.rest_child.lock().unwrap() = Some(child);
        Ok(())
    }

    async fn start_socket(self: &Arc<Self>) -> Result<()> {
        if self.socket_child.lock().unwrap().is_some() {
            return Ok(());
        }

        let settings = self.settings.lock().unwrap().clone();
        let node_settings = self.node_settings.lock().unwrap().clone();

        if !settings.enabled {
            return Ok(());
        }

        if Self::grpc_address_from_settings(&node_settings).is_none() {
            log_warn!("self-hosted-explorer: gRPC is disabled; socket server not started");
            return Ok(());
        }

        let socket_port = settings.effective_explorer_socket_port(node_settings.network);
        if !Self::port_is_available(&settings.api_bind, socket_port) {
            let msg = format!(
                "Socket port already in use on {}:{}; refusing to start socket server",
                settings.api_bind, socket_port
            );
            log_warn!("self-hosted-explorer: {msg}");
            self.socket_logs.push("ERROR", &msg);
            return Ok(());
        }

        let root = match Self::find_server_root("kaspa-socket-server") {
            Some(root) => root,
            None => {
                log_warn!("self-hosted-explorer: kaspa-socket-server not found");
                return Ok(());
            }
        };

        let mut cmd = match Self::build_command(&root, &settings.api_bind, socket_port) {
            Some(cmd) => cmd,
            None => {
                log_warn!(
                    "self-hosted-explorer: python runtime not found; socket server not started"
                );
                return Ok(());
            }
        };

        cmd.current_dir(&root);
        Self::apply_common_env(&mut cmd, &settings, &node_settings);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                let err = Error::NodeStartupError(err);
                log_warn!("self-hosted-explorer: failed to start socket server ({err})");
                return Err(err);
            }
        };

        self.socket_logs.push(
            "INFO",
            &format!(
                "Socket server listening on {}:{}",
                settings.api_bind, socket_port
            ),
        );

        let logs_info = self.socket_logs.clone();
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let level = Self::detect_log_level(&line, "INFO");
                    match level {
                        "ERROR" => log_warn!("self-hosted-socket: {line}"),
                        "WARN" => log_warn!("self-hosted-socket: {line}"),
                        _ => log_info!("self-hosted-socket: {line}"),
                    }
                    logs_info.push(level, &line);
                }
            });
        }

        let logs_warn = self.socket_logs.clone();
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let level = Self::detect_log_level(&line, "WARN");
                    match level {
                        "ERROR" => log_warn!("self-hosted-socket: {line}"),
                        "WARN" => log_warn!("self-hosted-socket: {line}"),
                        _ => log_info!("self-hosted-socket: {line}"),
                    }
                    logs_warn.push(level, &line);
                }
            });
        }

        *self.socket_child.lock().unwrap() = Some(child);
        Ok(())
    }

    async fn stop_rest(&self) -> Result<()> {
        let child = self.rest_child.lock().unwrap().take();
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

    async fn stop_socket(&self) -> Result<()> {
        let child = self.socket_child.lock().unwrap().take();
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

    #[cfg(unix)]
    async fn terminate_process_tree(child: &mut Child) {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;

        if let Some(pid) = child.id() {
            let pgid = Pid::from_raw(pid as i32);
            let _ = killpg(pgid, Signal::SIGTERM);
            if timeout(Duration::from_secs(2), child.wait()).await.is_ok() {
                return;
            }
            let _ = killpg(pgid, Signal::SIGKILL);
            let _ = timeout(Duration::from_secs(2), child.wait()).await;
            sleep(Duration::from_millis(100)).await;
        } else {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }

    async fn start_all(self: &Arc<Self>) -> Result<()> {
        let _ = self.start_rest().await;
        let _ = self.start_socket().await;
        Ok(())
    }

    async fn stop_all(&self) -> Result<()> {
        let _ = self.stop_rest().await;
        let _ = self.stop_socket().await;
        Ok(())
    }
}

#[async_trait]
impl Service for SelfHostedExplorerService {
    fn name(&self) -> &'static str {
        "self-hosted-explorer"
    }

    async fn spawn(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        tokio::spawn(async move {
            if this.is_enabled.load(Ordering::SeqCst) {
                let _ = this.start_all().await;
            }

            loop {
                select! {
                    msg = this.service_events.receiver.recv().fuse() => {
                        match msg {
                            Ok(SelfHostedExplorerEvents::Enable) => {
                                let was_enabled = this.is_enabled.swap(true, Ordering::SeqCst);
                                if !was_enabled {
                                    let _ = this.start_all().await;
                                }
                            }
                            Ok(SelfHostedExplorerEvents::Disable) => {
                                let was_enabled = this.is_enabled.swap(false, Ordering::SeqCst);
                                if was_enabled {
                                    let _ = this.stop_all().await;
                                }
                            }
                            Ok(SelfHostedExplorerEvents::UpdateSettings(settings)) => {
                                *this.settings.lock().unwrap() = settings;
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let _ = this.stop_all().await;
                                    let _ = this.start_all().await;
                                }
                            }
                            Ok(SelfHostedExplorerEvents::UpdateNodeSettings(settings)) => {
                                *this.node_settings.lock().unwrap() = settings;
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let _ = this.stop_all().await;
                                    let _ = this.start_all().await;
                                }
                            }
                            Ok(SelfHostedExplorerEvents::Exit) | Err(_) => {
                                let _ = this.stop_all().await;
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
            .try_send(SelfHostedExplorerEvents::Exit);
    }

    async fn join(self: Arc<Self>) -> Result<()> {
        self.task_ctl.recv().await.unwrap();
        Ok(())
    }
}
