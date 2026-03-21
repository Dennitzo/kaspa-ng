use crate::imports::*;
use crate::runtime::services::{LogStore, LogStores};
use std::net::TcpListener;
use std::process::Stdio;
use std::time::{Duration as StdDuration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

pub enum SelfHostedExplorerEvents {
    Enable,
    Disable,
    RestartRest,
    RestartSocket,
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
    rest_start_cooldown_until: Mutex<Option<Instant>>,
    socket_start_cooldown_until: Mutex<Option<Instant>>,
}

impl SelfHostedExplorerService {
    const REST_REQUIRED_PY_MODULES: &'static [&'static str] = &[
        "uvicorn",
        "fastapi_utils",
        "typing_inspect",
        "sqlalchemy",
        "asyncpg",
        "grpc",
        "kaspa_script_address",
        "kaspa",
    ];
    const REST_REQUIRED_PIP_PACKAGES: &'static [&'static str] = &[
        "uvicorn",
        "fastapi",
        "fastapi-utils",
        "typing-inspect",
        "sqlalchemy",
        "grpcio",
        "grpcio-tools",
        "requests",
        "websockets",
        "asyncpg",
        "cachetools",
        "aiohttp",
        "aiocache",
        "psycopg2-binary",
        "waitress",
        "starlette",
        "greenlet",
        "kaspa-script-address",
        "kaspa",
    ];
    const SOCKET_REQUIRED_PY_MODULES: &'static [&'static str] = &[
        "uvicorn",
        "fastapi_utils",
        "typing_inspect",
        "sqlalchemy",
        "socketio",
        "asyncpg",
        "grpc",
    ];
    const SOCKET_REQUIRED_PIP_PACKAGES: &'static [&'static str] = &[
        "uvicorn",
        "fastapi",
        "fastapi-utils",
        "typing-inspect",
        "sqlalchemy",
        "python-socketio",
        "grpcio",
        "grpcio-tools",
        "requests",
        "websockets",
        "asyncpg",
        "cachetools",
        "psycopg2-binary",
    ];
    fn default_grpc_port_for_network(network: Network) -> u16 {
        crate::settings::node_grpc_port_for_network(network)
    }

    fn running_from_macos_bundle() -> bool {
        #[cfg(target_os = "macos")]
        {
            if let Ok(exe) = std::env::current_exe() {
                return exe.to_string_lossy().contains(".app/Contents/MacOS/");
            }
        }
        false
    }

    fn detect_log_level<'a>(line: &str, fallback: &'a str) -> &'a str {
        let upper = line.to_ascii_uppercase();
        if upper.contains("DEBUG") || upper.contains("[DEBUG]") || upper.contains("::DEBUG::") {
            return "INFO";
        }
        if upper.contains("CRITICAL")
            || upper.contains("[CRITICAL]")
            || upper.contains("ERROR")
            || upper.contains("[ERROR]")
        {
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
            is_enabled: AtomicBool::new(false),
            rest_logs: logs.rest,
            socket_logs: logs.socket,
            rest_child: Mutex::new(None),
            socket_child: Mutex::new(None),
            rest_start_cooldown_until: Mutex::new(None),
            socket_start_cooldown_until: Mutex::new(None),
        }
    }

    fn explorer_settings_changed(
        prev: &SelfHostedSettings,
        next: &SelfHostedSettings,
        node: &NodeSettings,
    ) -> bool {
        prev.enabled != next.enabled
            || prev.api_bind != next.api_bind
            || prev.db_host != next.db_host
            || prev.db_name != next.db_name
            || prev.db_user != next.db_user
            || prev.db_password != next.db_password
            || prev.effective_explorer_rest_port(node.network)
                != next.effective_explorer_rest_port(node.network)
            || prev.effective_explorer_socket_port(node.network)
                != next.effective_explorer_socket_port(node.network)
            || prev.effective_db_port(node.network) != next.effective_db_port(node.network)
    }

    fn explorer_node_settings_changed(prev: &NodeSettings, next: &NodeSettings) -> bool {
        prev.network != next.network
            || prev.enable_grpc != next.enable_grpc
            || prev.grpc_network_interface != next.grpc_network_interface
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

    pub fn restart_rest(&self) {
        self.service_events
            .try_send(SelfHostedExplorerEvents::RestartRest)
            .unwrap();
    }

    pub fn restart_socket(&self) {
        self.service_events
            .try_send(SelfHostedExplorerEvents::RestartSocket)
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
            Some(format!(
                "{addr}:{}",
                Self::default_grpc_port_for_network(settings.network)
            ))
        }
    }

    fn network_type(settings: &NodeSettings) -> &'static str {
        let _ = settings;
        "mainnet"
    }

    fn network_id(settings: &NodeSettings) -> &'static str {
        let _ = settings;
        "mainnet"
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
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(bin_name);
            if candidate.exists() {
                return Some(candidate);
            }

            #[cfg(windows)]
            {
                if Path::new(bin_name).extension().is_none() {
                    let pathext = std::env::var_os("PATHEXT")
                        .map(|value| {
                            value
                                .to_string_lossy()
                                .split(';')
                                .map(|s| s.trim().to_ascii_lowercase())
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_else(|| {
                            vec![
                                ".exe".to_string(),
                                ".cmd".to_string(),
                                ".bat".to_string(),
                                ".com".to_string(),
                            ]
                        });

                    for ext in pathext {
                        let ext = ext.trim_start_matches('.');
                        let candidate = dir.join(format!("{bin_name}.{ext}"));
                        if candidate.exists() {
                            return Some(candidate);
                        }
                    }
                }
            }
        }

        None
    }

    fn find_executable(bin_name: &str, extra_dirs: &[PathBuf]) -> Option<PathBuf> {
        for dir in extra_dirs {
            let candidate = dir.join(bin_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        Self::find_in_path(bin_name)
    }

    fn find_poetry() -> Option<PathBuf> {
        let mut extra_dirs: Vec<PathBuf> = Vec::new();

        #[cfg(target_os = "macos")]
        {
            extra_dirs.extend([
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ]);

            if let Some(home) = workflow_core::dirs::home_dir() {
                extra_dirs.push(home.join(".local/bin"));
                extra_dirs.push(home.join("Library/Application Support/pypoetry/venv/bin"));
            }
        }

        #[cfg(target_os = "linux")]
        {
            extra_dirs.extend([PathBuf::from("/usr/local/bin"), PathBuf::from("/usr/bin")]);

            if let Some(home) = workflow_core::dirs::home_dir() {
                extra_dirs.push(home.join(".local/bin"));
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(home) = workflow_core::dirs::home_dir() {
                extra_dirs.push(home.join("AppData/Roaming/Python/Scripts"));
                extra_dirs.push(home.join("AppData/Local/pypoetry/Cache/virtualenvs"));
            }
        }

        Self::find_executable("poetry", &extra_dirs)
    }

    fn find_pipenv() -> Option<PathBuf> {
        let mut extra_dirs: Vec<PathBuf> = Vec::new();

        #[cfg(target_os = "macos")]
        {
            extra_dirs.extend([
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ]);

            if let Some(home) = workflow_core::dirs::home_dir() {
                extra_dirs.push(home.join(".local/bin"));
            }
        }

        #[cfg(target_os = "linux")]
        {
            extra_dirs.extend([PathBuf::from("/usr/local/bin"), PathBuf::from("/usr/bin")]);

            if let Some(home) = workflow_core::dirs::home_dir() {
                extra_dirs.push(home.join(".local/bin"));
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(home) = workflow_core::dirs::home_dir() {
                extra_dirs.push(home.join("AppData/Roaming/Python/Scripts"));
            }
        }

        Self::find_executable("pipenv", &extra_dirs)
    }

    fn discover_python_binaries_in_path() -> Vec<PathBuf> {
        #[cfg(windows)]
        {
            return Vec::new();
        }

        #[cfg(not(windows))]
        {
            let mut candidates = Vec::new();
            let Some(path_var) = std::env::var_os("PATH") else {
                return candidates;
            };

            for dir in std::env::split_paths(&path_var) {
                let Ok(entries) = std::fs::read_dir(dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };

                    let is_python3 = name
                        .strip_prefix("python3")
                        .map(|suffix| suffix.is_empty() || suffix.starts_with('.'))
                        .unwrap_or(false);
                    if is_python3 || name == "python" {
                        candidates.push(path);
                    }
                }
            }

            candidates
        }
    }

    fn python_version_for_executable(executable: &Path) -> Option<(u32, u32)> {
        let mut cmd = std::process::Command::new(executable);
        Self::apply_no_window_for_std_command(&mut cmd);
        let output = cmd
            .arg("-c")
            .arg("import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let mut parts = version.split('.');
        let major = parts.next().and_then(|p| p.parse::<u32>().ok())?;
        let minor = parts.next().and_then(|p| p.parse::<u32>().ok())?;
        Some((major, minor))
    }

    fn python_minor_preference(minor: u32) -> u8 {
        // Prefer versions that are currently compatible with the self-hosted Python deps.
        // Python 3.14+ can break `kaspa` installs in some environments, so keep it as fallback.
        if (10..=13).contains(&minor) { 1 } else { 0 }
    }

    fn python_version_is_preferred(version: (u32, u32)) -> bool {
        version.0 == 3 && Self::python_minor_preference(version.1) > 0
    }

    fn allow_system_python_fallback() -> bool {
        std::env::var("KASPA_NG_ALLOW_SYSTEM_PYTHON")
            .map(|value| {
                value == "1"
                    || value.eq_ignore_ascii_case("true")
                    || value.eq_ignore_ascii_case("yes")
            })
            .unwrap_or(false)
    }

    fn local_python_runtime_roots() -> Vec<PathBuf> {
        let mut roots = Vec::new();

        if let Ok(root) = std::env::var("KASPA_NG_PYTHON_RUNTIME_ROOT") {
            let trimmed = root.trim();
            if !trimmed.is_empty() {
                roots.push(PathBuf::from(trimmed));
            }
        }

        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            roots.push(dir.join("python"));
            roots.push(dir.join("Resources").join("python"));
            if let Some(contents) = dir.parent() {
                roots.push(contents.join("Resources").join("python"));
            }
            for ancestor in dir.ancestors().skip(1).take(4) {
                roots.push(ancestor.join("python"));
                roots.push(ancestor.join("Resources").join("python"));
            }
        }

        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd.join("python"));
            for ancestor in cwd.ancestors().skip(1).take(4) {
                roots.push(ancestor.join("python"));
            }
        }

        let mut deduped = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for root in roots {
            let key = root.canonicalize().unwrap_or_else(|_| root.clone());
            if seen.insert(key) {
                deduped.push(root);
            }
        }

        deduped
    }

    fn local_python_binary_candidates() -> Vec<PathBuf> {
        let mut candidates = Vec::new();

        if let Ok(bin) = std::env::var("KASPA_NG_PYTHON_BIN") {
            let trimmed = bin.trim();
            if !trimmed.is_empty() {
                candidates.push(PathBuf::from(trimmed));
            }
        }

        for root in Self::local_python_runtime_roots() {
            #[cfg(target_os = "windows")]
            {
                candidates.extend([
                    root.join("python.exe"),
                    root.join("bin").join("python.exe"),
                    root.join("Scripts").join("python.exe"),
                ]);
            }
            #[cfg(not(target_os = "windows"))]
            {
                candidates.extend([root.join("bin/python3"), root.join("bin/python")]);
            }
        }

        candidates
    }

    fn ranked_python_candidates(min_minor: u32) -> Vec<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        candidates.extend(Self::local_python_binary_candidates());

        if Self::allow_system_python_fallback() {
            #[cfg(target_os = "macos")]
            {
                candidates.extend(
                    [
                        "/opt/homebrew/bin/python3",
                        "/usr/local/bin/python3",
                        "/opt/homebrew/opt/python/bin/python3",
                        "/usr/local/opt/python/bin/python3",
                    ]
                    .into_iter()
                    .map(PathBuf::from),
                );

                if let Some(home) = workflow_core::dirs::home_dir() {
                    candidates.push(home.join(".local/bin/python3"));
                }
            }

            #[cfg(target_os = "linux")]
            {
                candidates.extend(
                    ["/usr/local/bin/python3", "/usr/bin/python3"]
                        .into_iter()
                        .map(PathBuf::from),
                );

                if let Some(home) = workflow_core::dirs::home_dir() {
                    candidates.push(home.join(".local/bin/python3"));
                }
            }

            #[cfg(target_os = "windows")]
            {
                if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
                    let base = PathBuf::from(local_app_data)
                        .join("Programs")
                        .join("Python");
                    candidates.extend([
                        base.join("Python314").join("python.exe"),
                        base.join("Python313").join("python.exe"),
                        base.join("Python312").join("python.exe"),
                        base.join("Python311").join("python.exe"),
                        base.join("Python310").join("python.exe"),
                    ]);
                }

                if let Some(program_files) = std::env::var_os("ProgramFiles") {
                    let base = PathBuf::from(program_files).join("Python");
                    candidates.extend([
                        base.join("Python314").join("python.exe"),
                        base.join("Python313").join("python.exe"),
                        base.join("Python312").join("python.exe"),
                        base.join("Python311").join("python.exe"),
                        base.join("Python310").join("python.exe"),
                    ]);
                }

                if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
                    let base = PathBuf::from(program_files_x86).join("Python");
                    candidates.extend([
                        base.join("Python314").join("python.exe"),
                        base.join("Python313").join("python.exe"),
                        base.join("Python312").join("python.exe"),
                        base.join("Python311").join("python.exe"),
                        base.join("Python310").join("python.exe"),
                    ]);
                }

                candidates.extend(
                    [
                        "C:\\Python314\\python.exe",
                        "C:\\Python313\\python.exe",
                        "C:\\Python312\\python.exe",
                        "C:\\Python311\\python.exe",
                        "C:\\Python310\\python.exe",
                        "C:\\Windows\\py.exe",
                    ]
                    .into_iter()
                    .map(PathBuf::from),
                );
            }

            if let Some(path_python3) = Self::find_in_path("python3") {
                candidates.push(path_python3);
            }
            if let Some(path_python) = Self::find_in_path("python") {
                candidates.push(path_python);
            }

            #[cfg(windows)]
            {
                if let Some(path_py) = Self::find_in_path("py") {
                    candidates.push(path_py);
                }
            }

            candidates.extend(Self::discover_python_binaries_in_path());
        }

        let mut ranked: Vec<(u32, u32, PathBuf)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for candidate in candidates {
            if !candidate.exists() {
                continue;
            }

            let dedup_key = candidate
                .canonicalize()
                .unwrap_or_else(|_| candidate.clone());
            if !seen.insert(dedup_key) {
                continue;
            }

            let Some((major, minor)) = Self::python_version_for_executable(&candidate) else {
                continue;
            };

            if major != 3 || minor < min_minor {
                continue;
            }

            ranked.push((major, minor, candidate));
        }

        ranked.sort_by(|a, b| {
            Self::python_minor_preference(b.1)
                .cmp(&Self::python_minor_preference(a.1))
                .then(b.0.cmp(&a.0))
                .then(b.1.cmp(&a.1))
        });
        ranked.into_iter().map(|(_, _, path)| path).collect()
    }

    fn find_python() -> Option<PathBuf> {
        Self::ranked_python_candidates(0).into_iter().next()
    }

    fn find_pipenv_python() -> Option<PathBuf> {
        Self::ranked_python_candidates(10).into_iter().next()
    }

    fn find_poetry_compatible_python() -> Option<PathBuf> {
        Self::ranked_python_candidates(10).into_iter().next()
    }

    fn find_venv_python(root: &Path) -> Option<PathBuf> {
        let candidates = [
            root.join(".venv/bin/python3"),
            root.join(".venv/bin/python"),
            root.join(".venv/Scripts/python.exe"),
        ];
        candidates.into_iter().find(|path| path.exists())
    }

    fn find_poetry_cached_venv_python(root: &Path) -> Option<PathBuf> {
        let project_name = root.file_name()?.to_string_lossy().to_string();
        let home = workflow_core::dirs::home_dir()?;

        let cache_dirs = [
            home.join("Library/Caches/pypoetry/virtualenvs"),
            home.join(".cache/pypoetry/virtualenvs"),
        ];

        for cache_dir in cache_dirs {
            let Ok(entries) = std::fs::read_dir(cache_dir) else {
                continue;
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };

                if !name.starts_with(&project_name) {
                    continue;
                }

                let candidates = [path.join("bin/python"), path.join("bin/python3")];
                if let Some(python) = candidates.into_iter().find(|p| p.exists()) {
                    return Some(python);
                }
            }
        }

        None
    }

    fn create_local_venv(root: &Path, bootstrap_python: &Path, venv_dir: &Path) -> bool {
        let mut cmd = std::process::Command::new(bootstrap_python);
        Self::apply_no_window_for_std_command(&mut cmd);
        cmd.current_dir(root).arg("-m").arg("venv").arg("--clear");
        #[cfg(not(windows))]
        {
            // Prefer copied interpreters to avoid hard links to build-host python paths.
            cmd.arg("--copies");
        }
        cmd.arg(venv_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if matches!(cmd.status(), Ok(status) if status.success()) {
            return true;
        }

        #[cfg(not(windows))]
        {
            let mut fallback = std::process::Command::new(bootstrap_python);
            Self::apply_no_window_for_std_command(&mut fallback);
            fallback
                .current_dir(root)
                .arg("-m")
                .arg("venv")
                .arg("--clear")
                .arg(venv_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if matches!(fallback.status(), Ok(status) if status.success()) {
                return true;
            }
        }

        false
    }

    fn ensure_local_venv_python(
        root: &Path,
        required_modules: &[&str],
        required_pip_packages: &[&str],
    ) -> Option<PathBuf> {
        let venv_dir = root.join(".venv");
        let candidates = Self::ranked_python_candidates(10);

        for bootstrap_python in candidates {
            if !Self::create_local_venv(root, &bootstrap_python, &venv_dir) {
                continue;
            }

            let Some(venv_python) = Self::find_venv_python(root) else {
                continue;
            };

            Self::ensure_python_modules_for_python(
                &venv_python,
                required_modules,
                required_pip_packages,
            );
            if Self::python_modules_available_for_python(&venv_python, required_modules) {
                return Some(venv_python);
            }
        }

        None
    }

    fn find_server_root(name: &str) -> Option<PathBuf> {
        let mut candidates = Vec::new();
        let is_macos_bundle = Self::running_from_macos_bundle();

        if let Ok(root) = std::env::var("KASPA_NG_EXPLORER_SERVERS_ROOT") {
            candidates.push(PathBuf::from(root).join(name));
        }

        if !is_macos_bundle && let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join(name));
            for ancestor in cwd.ancestors().skip(1).take(4) {
                candidates.push(ancestor.join(name));
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join(name));
                candidates.push(dir.join("Resources").join(name));
                if is_macos_bundle {
                    if let Some(contents) = dir.parent() {
                        candidates.push(contents.join("Resources").join(name));
                    }
                } else {
                    for ancestor in dir.ancestors().skip(1).take(4) {
                        candidates.push(ancestor.join(name));
                        candidates.push(ancestor.join("Resources").join(name));
                    }
                }
            }
        }

        candidates
            .into_iter()
            .find(|dir| dir.join("main.py").exists())
            .map(|dir| dir.canonicalize().unwrap_or(dir))
    }

    fn python_requirements_for_root(
        root: &Path,
    ) -> (&'static [&'static str], &'static [&'static str]) {
        let is_socket = root
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "kaspa-socket-server")
            .unwrap_or(false);
        if is_socket {
            (
                Self::SOCKET_REQUIRED_PY_MODULES,
                Self::SOCKET_REQUIRED_PIP_PACKAGES,
            )
        } else {
            (
                Self::REST_REQUIRED_PY_MODULES,
                Self::REST_REQUIRED_PIP_PACKAGES,
            )
        }
    }

    fn build_command(root: &Path, bind: &str, port: u16) -> Option<Command> {
        let (required_modules, required_pip_packages) = Self::python_requirements_for_root(root);

        if let Some(venv_python) = Self::find_venv_python(root) {
            let modules_ready =
                Self::python_modules_available_for_python(&venv_python, required_modules);
            if !modules_ready {
                let preferred = Self::python_version_for_executable(&venv_python)
                    .map(Self::python_version_is_preferred)
                    .unwrap_or(true);
                if preferred {
                    Self::ensure_python_modules_for_python(
                        &venv_python,
                        required_modules,
                        required_pip_packages,
                    );
                } else {
                    log_info!(
                        "self-hosted-explorer: existing venv python is newer than preferred; skipping in-place module bootstrap for {}",
                        venv_python.display()
                    );
                }
            }
            if Self::python_modules_available_for_python(&venv_python, required_modules) {
                let mut cmd = Command::new(&venv_python);
                #[cfg(windows)]
                {
                    // gunicorn depends on fcntl and is not supported on Windows.
                    cmd.arg("-m")
                        .arg("uvicorn")
                        .arg("main:app")
                        .arg("--host")
                        .arg(bind)
                        .arg("--port")
                        .arg(port.to_string());
                }
                #[cfg(not(windows))]
                {
                    if Self::python_module_available_for_python(&venv_python, "gunicorn") {
                        let bind_arg = format!("{bind}:{port}");
                        cmd.arg("-m")
                            .arg("gunicorn")
                            .arg("-w")
                            .arg("1")
                            .arg("-k")
                            .arg("uvicorn.workers.UvicornWorker")
                            .arg("main:app")
                            .arg("-b")
                            .arg(&bind_arg);
                    } else {
                        cmd.arg("-m")
                            .arg("uvicorn")
                            .arg("main:app")
                            .arg("--host")
                            .arg(bind)
                            .arg("--port")
                            .arg(port.to_string());
                    }
                }
                return Some(cmd);
            }
        }
        if root.join("pyproject.toml").exists() {
            if let Some(python) = Self::find_poetry_cached_venv_python(root) {
                if Self::python_modules_available_for_python(&python, required_modules) {
                    let mut cmd = Command::new(python);
                    cmd.arg("-m")
                        .arg("uvicorn")
                        .arg("main:app")
                        .arg("--host")
                        .arg(bind)
                        .arg("--port")
                        .arg(port.to_string());
                    return Some(cmd);
                }
            }

            if let Some(poetry) = Self::find_poetry() {
                if let Some(py) = Self::find_poetry_compatible_python() {
                    let mut cmd = std::process::Command::new(&poetry);
                    Self::apply_no_window_for_std_command(&mut cmd);
                    let _ = cmd.current_dir(root).arg("env").arg("use").arg(py).status();
                }

                // Ensure runtime deps exist in the selected poetry env.
                if !Self::python_modules_available(
                    root,
                    &poetry,
                    &["run", "python"],
                    required_modules,
                ) {
                    let mut cmd = std::process::Command::new(&poetry);
                    Self::apply_no_window_for_std_command(&mut cmd);
                    let _ = cmd
                        .current_dir(root)
                        .arg("install")
                        .arg("--only")
                        .arg("main")
                        .arg("--no-root")
                        .arg("--no-interaction")
                        .status();
                }
                if !Self::python_modules_available(
                    root,
                    &poetry,
                    &["run", "python"],
                    required_modules,
                ) {
                    let mut cmd = std::process::Command::new(&poetry);
                    Self::apply_no_window_for_std_command(&mut cmd);
                    cmd.current_dir(root)
                        .arg("run")
                        .arg("python")
                        .arg("-m")
                        .arg("pip")
                        .arg("install");
                    for pkg in required_pip_packages {
                        cmd.arg(pkg);
                    }
                    let _ = cmd.status();
                }
                if Self::python_modules_available(
                    root,
                    &poetry,
                    &["run", "python"],
                    required_modules,
                ) {
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
            if let Some(pipenv) = Self::find_pipenv() {
                if let Some(python) = Self::find_pipenv_python() {
                    let mut cmd = std::process::Command::new(&pipenv);
                    Self::apply_no_window_for_std_command(&mut cmd);
                    let _ = cmd.current_dir(root).arg("--python").arg(python).status();
                }

                // Ensure runtime deps are available in the pipenv environment.
                if !Self::python_modules_available(
                    root,
                    &pipenv,
                    &["run", "python"],
                    required_modules,
                ) {
                    let mut cmd = std::process::Command::new(&pipenv);
                    Self::apply_no_window_for_std_command(&mut cmd);
                    let install_status = cmd
                        .current_dir(root)
                        .arg("install")
                        .arg("--deploy")
                        .status();
                    if !matches!(install_status, Ok(status) if status.success()) {
                        let mut cmd = std::process::Command::new(&pipenv);
                        Self::apply_no_window_for_std_command(&mut cmd);
                        let _ = cmd.current_dir(root).arg("install").status();
                    }
                }
                if !Self::python_modules_available(
                    root,
                    &pipenv,
                    &["run", "python"],
                    required_modules,
                ) {
                    let mut cmd = std::process::Command::new(&pipenv);
                    Self::apply_no_window_for_std_command(&mut cmd);
                    cmd.current_dir(root)
                        .arg("run")
                        .arg("python")
                        .arg("-m")
                        .arg("pip")
                        .arg("install");
                    for pkg in required_pip_packages {
                        cmd.arg(pkg);
                    }
                    let _ = cmd.status();
                }
                if Self::python_modules_available(
                    root,
                    &pipenv,
                    &["run", "python"],
                    required_modules,
                ) {
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
        }

        if let Some(venv_python) =
            Self::ensure_local_venv_python(root, required_modules, required_pip_packages)
        {
            let mut cmd = Command::new(venv_python);
            cmd.arg("-m")
                .arg("uvicorn")
                .arg("main:app")
                .arg("--host")
                .arg(bind)
                .arg("--port")
                .arg(port.to_string());
            return Some(cmd);
        }

        let python = Self::find_python()?;
        Self::ensure_python_modules_for_python(&python, required_modules, required_pip_packages);
        if !Self::python_modules_available_for_python(&python, required_modules) {
            return None;
        }
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

    fn ensure_python_modules_for_python(python: &Path, modules: &[&str], pip_packages: &[&str]) {
        if Self::python_modules_available_for_python(python, modules) {
            return;
        }
        if !pip_packages.is_empty() {
            let mut ensurepip_cmd = std::process::Command::new(python);
            Self::apply_no_window_for_std_command(&mut ensurepip_cmd);
            let _ = ensurepip_cmd
                .arg("-m")
                .arg("ensurepip")
                .arg("--upgrade")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();

            let mut cmd = std::process::Command::new(python);
            Self::apply_no_window_for_std_command(&mut cmd);
            cmd.arg("-m").arg("pip").arg("install");
            for pkg in pip_packages {
                cmd.arg(pkg);
            }
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
            let _ = cmd.status();

            // `kaspa` currently advertises `Requires-Python <=3.14`, which rejects patch
            // versions like 3.14.3. On those runtimes, force-install a known wheel variant.
            let kaspa_requested = pip_packages
                .iter()
                .any(|pkg| pkg.trim().eq_ignore_ascii_case("kaspa"));
            let py_is_314_or_newer = Self::python_version_for_executable(python)
                .map(|(major, minor)| major == 3 && minor >= 14)
                .unwrap_or(false);
            if kaspa_requested
                && py_is_314_or_newer
                && !Self::python_module_available_for_python(python, "kaspa")
            {
                let mut fallback_cmd = std::process::Command::new(python);
                Self::apply_no_window_for_std_command(&mut fallback_cmd);
                fallback_cmd
                    .arg("-m")
                    .arg("pip")
                    .arg("install")
                    .arg("--ignore-requires-python")
                    .arg("kaspa==1.1.0")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let _ = fallback_cmd.status();
            }
        }
    }

    fn python_module_available(
        root: &Path,
        runner: &Path,
        runner_args: &[&str],
        module: &str,
    ) -> bool {
        let mut cmd = std::process::Command::new(runner);
        Self::apply_no_window_for_std_command(&mut cmd);
        cmd.current_dir(root);
        for arg in runner_args {
            cmd.arg(arg);
        }
        cmd.arg("-c").arg(format!("import {module}"));
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        matches!(cmd.status(), Ok(status) if status.success())
    }

    fn python_modules_available(
        root: &Path,
        runner: &Path,
        runner_args: &[&str],
        modules: &[&str],
    ) -> bool {
        modules
            .iter()
            .all(|module| Self::python_module_available(root, runner, runner_args, module))
    }

    fn python_module_available_for_python(python: &Path, module: &str) -> bool {
        let mut cmd = std::process::Command::new(python);
        Self::apply_no_window_for_std_command(&mut cmd);
        cmd.arg("-c").arg(format!("import {module}"));
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        matches!(cmd.status(), Ok(status) if status.success())
    }

    fn python_modules_available_for_python(python: &Path, modules: &[&str]) -> bool {
        modules
            .iter()
            .all(|module| Self::python_module_available_for_python(python, module))
    }

    fn apply_no_window_for_std_command(_cmd: &mut std::process::Command) {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            _cmd.creation_flags(CREATE_NO_WINDOW);
        }
    }

    fn apply_common_env(cmd: &mut Command, settings: &SelfHostedSettings, node: &NodeSettings) {
        cmd.env("SQL_URI", Self::build_sql_uri(settings, node));
        if let Some(grpc) = Self::grpc_address_from_settings(node) {
            cmd.env("KASPAD_HOST_1", grpc);
        }
        // Enable optional REST dataset endpoints for self-hosted explorer mode.
        cmd.env("TRANSACTION_COUNT", "true");
        cmd.env("ADDRESSES_ACTIVE_COUNT", "true");
        cmd.env("ADDRESS_RANKINGS", "true");
        cmd.env("HASHRATE_HISTORY", "true");
        cmd.env("NETWORK_TYPE", Self::network_type(node));
        cmd.env("APP_NETWORK_ID", Self::network_id(node));
        cmd.env("DEBUG", "false");
        cmd.env("PYTHONUNBUFFERED", "1");
        if std::env::var_os("LANG").is_none() {
            cmd.env("LANG", "en_US.UTF-8");
        }
        if std::env::var_os("LC_ALL").is_none() {
            cmd.env("LC_ALL", "en_US.UTF-8");
        }
    }

    fn python_runtime_install_hint() -> Option<&'static str> {
        if !Self::allow_system_python_fallback() {
            return Some(
                "bundled Python runtime missing; provide ./python runtime next to the app or set KASPA_NG_PYTHON_RUNTIME_ROOT",
            );
        }
        #[cfg(target_os = "linux")]
        {
            return Some(
                "install Python 3 with venv/pip (Debian/Ubuntu: sudo apt install python3 python3-venv python3-pip)",
            );
        }
        #[cfg(target_os = "macos")]
        {
            return Some("install Python 3 (for example: brew install python)");
        }
        #[cfg(target_os = "windows")]
        {
            return Some("install Python 3.10+ (for example: winget install Python.Python.3.11)");
        }
        #[allow(unreachable_code)]
        None
    }

    fn python_runtime_missing_message(server_kind: &str) -> String {
        match Self::python_runtime_install_hint() {
            Some(hint) => {
                format!("python runtime not found; {server_kind} server not started ({hint})")
            }
            None => format!("python runtime not found; {server_kind} server not started"),
        }
    }

    fn port_is_available(bind: &str, port: u16) -> bool {
        let addr = format!("{bind}:{port}");
        TcpListener::bind(addr).is_ok()
    }

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

    fn probe_host_from_bind(bind: &str) -> String {
        let trimmed = bind.trim();
        if trimmed.is_empty() || trimmed == "0.0.0.0" || trimmed == "::" || trimmed == "[::]" {
            "127.0.0.1".to_string()
        } else {
            trimmed.to_string()
        }
    }

    async fn wait_for_tcp_healthy(host: &str, port: u16, deadline: StdDuration) -> bool {
        let started = Instant::now();
        while started.elapsed() < deadline {
            let connect = timeout(
                StdDuration::from_millis(900),
                TcpStream::connect((host, port)),
            )
            .await;
            if let Ok(Ok(_)) = connect {
                return true;
            }
            sleep(StdDuration::from_millis(250)).await;
        }
        false
    }

    async fn start_rest(self: &Arc<Self>) -> Result<()> {
        if self.rest_child.lock().unwrap().is_some() {
            return Ok(());
        }

        if let Some(until) = *self.rest_start_cooldown_until.lock().unwrap() {
            if Instant::now() < until {
                return Ok(());
            }
            *self.rest_start_cooldown_until.lock().unwrap() = None;
        }

        let settings = self.settings.lock().unwrap().clone();
        let node_settings = self.node_settings.lock().unwrap().clone();

        if !settings.enabled {
            self.rest_logs.push(
                "INFO",
                "self-hosted explorer disabled; REST server not started",
            );
            return Ok(());
        }

        if Self::grpc_address_from_settings(&node_settings).is_none() {
            log_warn!("self-hosted-explorer: gRPC is disabled; REST server not started");
            self.rest_logs
                .push("WARN", "gRPC is disabled; REST server not started");
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
                self.rest_logs.push("ERROR", "kaspa-rest-server not found");
                return Ok(());
            }
        };
        self.rest_logs
            .push("INFO", &format!("REST server root: {}", root.display()));

        let mut cmd = match Self::build_command(&root, &settings.api_bind, rest_port) {
            Some(cmd) => cmd,
            None => {
                let msg = Self::python_runtime_missing_message("REST");
                log_warn!("self-hosted-explorer: {msg}");
                self.rest_logs.push("ERROR", &msg);
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
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                let err = Error::NodeStartupError(err);
                let cooldown = StdDuration::from_secs(30);
                *self.rest_start_cooldown_until.lock().unwrap() = Some(Instant::now() + cooldown);
                log_warn!("self-hosted-explorer: failed to start rest server ({err})");
                self.rest_logs.push(
                    "WARN",
                    &format!(
                        "REST server start failed; suppressing restart attempts for {}s",
                        cooldown.as_secs()
                    ),
                );
                return Err(err);
            }
        };

        *self.rest_start_cooldown_until.lock().unwrap() = None;

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

        if let Some(until) = *self.socket_start_cooldown_until.lock().unwrap() {
            if Instant::now() < until {
                return Ok(());
            }
            *self.socket_start_cooldown_until.lock().unwrap() = None;
        }

        let settings = self.settings.lock().unwrap().clone();
        let node_settings = self.node_settings.lock().unwrap().clone();

        if !settings.enabled {
            self.socket_logs.push(
                "INFO",
                "self-hosted explorer disabled; socket server not started",
            );
            return Ok(());
        }

        if Self::grpc_address_from_settings(&node_settings).is_none() {
            log_warn!("self-hosted-explorer: gRPC is disabled; socket server not started");
            self.socket_logs
                .push("WARN", "gRPC is disabled; socket server not started");
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
                self.socket_logs
                    .push("ERROR", "kaspa-socket-server not found");
                return Ok(());
            }
        };
        self.socket_logs
            .push("INFO", &format!("Socket server root: {}", root.display()));

        let mut cmd = match Self::build_command(&root, &settings.api_bind, socket_port) {
            Some(cmd) => cmd,
            None => {
                let msg = Self::python_runtime_missing_message("Socket");
                log_warn!("self-hosted-explorer: {msg}");
                self.socket_logs.push("ERROR", &msg);
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
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                let err = Error::NodeStartupError(err);
                let cooldown = StdDuration::from_secs(30);
                *self.socket_start_cooldown_until.lock().unwrap() = Some(Instant::now() + cooldown);
                log_warn!("self-hosted-explorer: failed to start socket server ({err})");
                self.socket_logs.push(
                    "WARN",
                    &format!(
                        "Socket server start failed; suppressing restart attempts for {}s",
                        cooldown.as_secs()
                    ),
                );
                return Err(err);
            }
        };

        *self.socket_start_cooldown_until.lock().unwrap() = None;

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
            if timeout(StdDuration::from_secs(2), child.wait())
                .await
                .is_ok()
            {
                return;
            }
            let _ = killpg(pgid, Signal::SIGKILL);
            let _ = timeout(StdDuration::from_secs(2), child.wait()).await;
            sleep(StdDuration::from_millis(100)).await;
        } else {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }

    async fn start_all(self: &Arc<Self>) -> Result<()> {
        let settings = self.settings.lock().unwrap().clone();
        let node_settings = self.node_settings.lock().unwrap().clone();
        let probe_host = Self::probe_host_from_bind(&settings.api_bind);
        let rest_port = settings.effective_explorer_rest_port(node_settings.network);
        let socket_port = settings.effective_explorer_socket_port(node_settings.network);

        let _ = self.start_rest().await;
        if self.rest_child.lock().unwrap().is_none() {
            self.socket_logs.push(
                "WARN",
                "Socket startup skipped because REST server is not running",
            );
            return Ok(());
        }
        if !Self::wait_for_tcp_healthy(&probe_host, rest_port, StdDuration::from_secs(30)).await {
            self.socket_logs.push(
                "WARN",
                &format!(
                    "Socket startup delayed: REST health check on {}:{} timed out",
                    probe_host, rest_port
                ),
            );
            return Ok(());
        }

        let _ = self.start_socket().await;
        if self.socket_child.lock().unwrap().is_some() {
            let _ =
                Self::wait_for_tcp_healthy(&probe_host, socket_port, StdDuration::from_secs(20))
                    .await;
        }
        Ok(())
    }

    async fn ensure_running(self: &Arc<Self>) {
        let rest_running = {
            let mut guard = self.rest_child.lock().unwrap();
            Self::child_is_running(&mut guard, "REST server", &self.rest_logs)
        };
        if !rest_running {
            let _ = self.start_rest().await;
        }

        let rest_running = {
            let mut guard = self.rest_child.lock().unwrap();
            Self::child_is_running(&mut guard, "REST server", &self.rest_logs)
        };
        if !rest_running {
            return;
        }

        let socket_running = {
            let mut guard = self.socket_child.lock().unwrap();
            Self::child_is_running(&mut guard, "socket server", &self.socket_logs)
        };
        if !socket_running {
            let _ = self.start_socket().await;
        }
    }

    async fn stop_all(&self) -> Result<()> {
        let _ = self.stop_rest().await;
        let _ = self.stop_socket().await;
        Ok(())
    }

    async fn restart_rest_only(self: &Arc<Self>) -> Result<()> {
        let _ = self.stop_rest().await;
        sleep(StdDuration::from_millis(350)).await;
        self.start_rest().await
    }

    async fn restart_socket_only(self: &Arc<Self>) -> Result<()> {
        let _ = self.stop_socket().await;
        sleep(StdDuration::from_millis(300)).await;
        self.start_socket().await
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
            let mut retry_tick = tokio::time::interval(StdDuration::from_secs(5));

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
                            Ok(SelfHostedExplorerEvents::RestartRest) => {
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let _ = this.restart_rest_only().await;
                                }
                            }
                            Ok(SelfHostedExplorerEvents::RestartSocket) => {
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let _ = this.restart_socket_only().await;
                                }
                            }
                            Ok(SelfHostedExplorerEvents::UpdateSettings(settings)) => {
                                let restart_needed = {
                                    let mut stored = this.settings.lock().unwrap();
                                    let current_node = this.node_settings.lock().unwrap().clone();
                                    let changed = Self::explorer_settings_changed(
                                        &stored,
                                        &settings,
                                        &current_node,
                                    );
                                    *stored = settings;
                                    changed
                                };
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    if restart_needed {
                                        let _ = this.stop_all().await;
                                        let _ = this.start_all().await;
                                    }
                                }
                            }
                            Ok(SelfHostedExplorerEvents::UpdateNodeSettings(settings)) => {
                                let restart_needed = {
                                    let mut stored = this.node_settings.lock().unwrap();
                                    let changed =
                                        Self::explorer_node_settings_changed(&stored, &settings);
                                    *stored = settings;
                                    changed
                                };
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    if restart_needed {
                                        let _ = this.stop_all().await;
                                        let _ = this.start_all().await;
                                    }
                                }
                            }
                            Ok(SelfHostedExplorerEvents::Exit) | Err(_) => {
                                let _ = this.stop_all().await;
                                break;
                            }
                        }
                    }
                    _ = retry_tick.tick().fuse() => {
                        if this.is_enabled.load(Ordering::SeqCst) {
                            this.ensure_running().await;
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
