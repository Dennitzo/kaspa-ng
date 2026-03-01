use crate::imports::*;
use crate::runtime::services::{LogStore, LogStores};
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

pub enum SelfHostedPostgresEvents {
    Enable,
    Disable,
    UpdateSettings(SelfHostedSettings),
    UpdateNodeSettings(NodeSettings),
    ResetDatabases,
    Exit,
}

pub struct SelfHostedPostgresService {
    pub application_events: ApplicationEventsChannel,
    pub service_events: Channel<SelfHostedPostgresEvents>,
    pub task_ctl: Channel<()>,
    pub settings: Mutex<SelfHostedSettings>,
    pub node_settings: Mutex<NodeSettings>,
    pub is_enabled: AtomicBool,
    logs: Arc<LogStore>,
    child: Mutex<Option<Child>>,
    last_restart_at: Mutex<Option<Instant>>,
    startup_restart_guard_until: Mutex<Option<Instant>>,
}

impl SelfHostedPostgresService {
    const STARTUP_RESTART_GUARD: Duration = Duration::from_secs(18);

    fn arm_startup_restart_guard(&self) {
        *self.startup_restart_guard_until.lock().unwrap() =
            Some(Instant::now() + Self::STARTUP_RESTART_GUARD);
    }

    fn clear_startup_restart_guard(&self) {
        *self.startup_restart_guard_until.lock().unwrap() = None;
    }

    fn should_skip_restart_due_to_startup_guard(
        &self,
        previous_port: u16,
        next_port: u16,
        reason: &str,
    ) -> bool {
        let mut guard = self.startup_restart_guard_until.lock().unwrap();
        if let Some(until) = *guard {
            if Instant::now() < until {
                self.logs.push(
                    "INFO",
                    &format!(
                        "ignoring early postgres restart ({reason}, port {} -> {}) during startup stabilisation",
                        previous_port, next_port
                    ),
                );
                return true;
            }
            *guard = None;
        }
        false
    }

    fn should_skip_restart_due_to_cooldown(
        &self,
        previous_port: u16,
        next_port: u16,
        reason: &str,
    ) -> bool {
        if let Some(last_restart_at) = *self.last_restart_at.lock().unwrap()
            && last_restart_at.elapsed() < Duration::from_secs(8)
        {
            self.logs.push(
                "INFO",
                &format!(
                    "ignoring transient postgres restart ({reason}, port {} -> {}) during restart cooldown",
                    previous_port, next_port
                ),
            );
            return true;
        }
        false
    }

    fn requires_restart_for_settings_change(
        previous: &SelfHostedSettings,
        next: &SelfHostedSettings,
        network: Network,
    ) -> bool {
        previous.effective_db_port(network) != next.effective_db_port(network)
            || previous.postgres_data_dir.trim() != next.postgres_data_dir.trim()
            || previous.db_host.trim() != next.db_host.trim()
            || previous.db_password != next.db_password
            || previous.db_user.trim() != next.db_user.trim()
            || Self::normalized_db_base_name(previous) != Self::normalized_db_base_name(next)
    }

    fn postgres_auto_install_attempted() -> &'static OnceLock<()> {
        static ONCE: OnceLock<()> = OnceLock::new();
        &ONCE
    }

    fn password_marker_path(data_dir: &Path) -> PathBuf {
        data_dir.join(".kaspa-ng-db-password")
    }

    fn write_password_marker(data_dir: &Path, password: &str) {
        let marker = Self::password_marker_path(data_dir);
        if let Err(err) = std::fs::write(&marker, password.as_bytes()) {
            log_warn!(
                "self-hosted-postgres: unable to write password marker '{}': {}",
                marker.display(),
                err
            );
        }
    }

    fn auth_recovery_lock_path(data_dir: &Path) -> PathBuf {
        let parent = data_dir.parent().unwrap_or(data_dir);
        let id = data_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("default");
        parent.join(format!(".kaspa-ng-auth-recovery-{id}.lock"))
    }

    fn marker_matches_password(data_dir: &Path, password: &str) -> bool {
        std::fs::read_to_string(Self::password_marker_path(data_dir))
            .map(|value| value.trim() == password.trim())
            .unwrap_or(false)
    }

    fn detect_log_level<'a>(line: &str, fallback: &'a str) -> &'a str {
        let upper = line.to_ascii_uppercase();
        // During crash recovery Postgres emits transient connection errors while still booting.
        // Suppress these noisy transient lines and rely on periodic redo progress logs.
        if upper.contains("DATABASE SYSTEM IS NOT YET ACCEPTING CONNECTIONS")
            || upper.contains("CONSISTENT RECOVERY STATE HAS NOT BEEN YET REACHED")
        {
            return "IGNORE";
        }
        // Expected transient lines during a normal self-managed shutdown/restart.
        if upper.contains("THE DATABASE SYSTEM IS SHUTTING DOWN")
            || upper.contains("TERMINATING CONNECTION DUE TO UNEXPECTED POSTMASTER EXIT")
        {
            return "WARN";
        }
        if upper.contains("FATAL:")
            || upper.contains("PANIC:")
            || upper.contains("ERROR:")
            || upper.contains("FEHLER:")
        {
            "ERROR"
        } else if upper.contains("WARNING:") || upper.contains("WARNUNG:") {
            "WARN"
        } else if upper.contains("LOG:") || upper.contains("HINT:") || upper.contains("TIPP:") {
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
            logs: logs.postgres,
            child: Mutex::new(None),
            last_restart_at: Mutex::new(None),
            startup_restart_guard_until: Mutex::new(None),
        }
    }

    pub fn enable(&self, enable: bool) {
        if enable {
            self.service_events
                .try_send(SelfHostedPostgresEvents::Enable)
                .unwrap();
        } else {
            self.service_events
                .try_send(SelfHostedPostgresEvents::Disable)
                .unwrap();
        }
    }

    pub fn update_settings(&self, settings: SelfHostedSettings) {
        self.service_events
            .try_send(SelfHostedPostgresEvents::UpdateSettings(settings))
            .unwrap();
    }

    pub fn update_node_settings(&self, settings: NodeSettings) {
        self.service_events
            .try_send(SelfHostedPostgresEvents::UpdateNodeSettings(settings))
            .unwrap();
    }

    pub fn reset_databases(&self) {
        let _ = self
            .service_events
            .try_send(SelfHostedPostgresEvents::ResetDatabases);
    }

    fn normalized_db_base_name(settings: &SelfHostedSettings) -> String {
        let mut base = settings.db_name.trim().to_string();
        if base.is_empty() {
            base = "kaspa".to_string();
        }
        base
    }

    fn all_network_db_names(settings: &SelfHostedSettings) -> Vec<String> {
        let base = Self::normalized_db_base_name(settings);
        vec![crate::settings::self_hosted_db_name_for_network(
            &base,
            Network::Mainnet,
        )]
    }

    fn resolve_data_dir(settings: &SelfHostedSettings, network: Network) -> Result<PathBuf> {
        if !settings.postgres_data_dir.trim().is_empty() {
            let mut path = PathBuf::from(settings.postgres_data_dir.trim());
            path.push(crate::settings::network_profile_slug(network));
            return Ok(path);
        }

        let default_storage_folder = kaspa_wallet_core::storage::local::default_storage_folder();
        let storage_folder = workflow_store::fs::resolve_path(default_storage_folder)?;
        Ok(storage_folder
            .join("self-hosted")
            .join("postgres")
            .join(crate::settings::network_profile_slug(network)))
    }

    fn postgres_bin_path(binary: &str) -> Result<PathBuf> {
        let bin_name = if cfg!(windows) {
            format!("{binary}.exe")
        } else {
            binary.to_string()
        };

        for bin_dir in Self::candidate_bin_dirs() {
            let candidate = bin_dir.join(&bin_name);
            if Self::is_runnable_binary(&candidate) {
                return Ok(candidate);
            }
        }

        if Self::postgres_auto_install_attempted().set(()).is_ok() {
            let _ = Self::attempt_auto_install_postgres();
            for bin_dir in Self::candidate_bin_dirs() {
                let candidate = bin_dir.join(&bin_name);
                if Self::is_runnable_binary(&candidate) {
                    return Ok(candidate);
                }
            }
        }

        if let Some(path) = Self::find_in_path(&bin_name) {
            return Ok(path);
        }

        Err(Error::Custom(format!(
            "postgres binary not found: {bin_name} (searched common install paths and PATH)"
        )))
    }

    fn is_runnable_binary(path: &Path) -> bool {
        if !path.exists() || !path.is_file() {
            return false;
        }

        let mut cmd = std::process::Command::new(path);
        Self::apply_no_window_for_std_command(&mut cmd);
        cmd.arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        matches!(cmd.status(), Ok(status) if status.success())
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

    fn candidate_bin_dirs() -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = Vec::new();

        if let Ok(exe) = std::env::current_exe()
            && let Some(exe_dir) = exe.parent()
        {
            dirs.push(exe_dir.join("postgres").join("bin"));

            #[cfg(target_os = "macos")]
            {
                // Kaspa-NG.app/Contents/MacOS -> Kaspa-NG.app/Contents/Resources/postgres/bin
                dirs.push(exe_dir.join("../Resources/postgres/bin"));
                dirs.push(exe_dir.join("../../Resources/postgres/bin"));
            }
        }

        if !Self::running_from_macos_bundle()
            && let Ok(cwd) = std::env::current_dir()
        {
            dirs.push(cwd.join("postgres").join("bin"));
        }

        if let Some(home) = workflow_core::dirs::home_dir() {
            dirs.push(home.join(".kaspa/self-hosted/postgres-runtime/bin"));
        }

        #[cfg(target_os = "macos")]
        {
            dirs.extend([
                PathBuf::from("/opt/homebrew/opt/postgresql@15/bin"),
                PathBuf::from("/usr/local/opt/postgresql@15/bin"),
                PathBuf::from("/opt/homebrew/opt/postgresql/bin"),
                PathBuf::from("/usr/local/opt/postgresql/bin"),
            ]);
        }

        #[cfg(target_os = "linux")]
        {
            dirs.extend([
                PathBuf::from("/usr/lib/postgresql/15/bin"),
                PathBuf::from("/usr/pgsql-15/bin"),
                PathBuf::from("/usr/local/pgsql/bin"),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
            ]);
        }

        #[cfg(target_os = "windows")]
        {
            dirs.extend([
                PathBuf::from("C:\\Program Files\\PostgreSQL\\15\\bin"),
                PathBuf::from("C:\\Program Files (x86)\\PostgreSQL\\15\\bin"),
            ]);
        }

        dirs
    }

    fn run_command_success(cmd: &mut std::process::Command) -> bool {
        Self::apply_no_window_for_std_command(cmd);
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

    fn attempt_auto_install_postgres() -> bool {
        #[cfg(target_os = "macos")]
        {
            if let Ok(prefix) = std::process::Command::new("brew")
                .args(["--prefix", "postgresql@15"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
                && prefix.status.success()
            {
                let path = String::from_utf8_lossy(&prefix.stdout).trim().to_string();
                if !path.is_empty() && PathBuf::from(path).join("bin/postgres").exists() {
                    return true;
                }
            }

            let mut list = std::process::Command::new("brew");
            list.args(["list", "postgresql@15"])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if Self::run_command_success(&mut list) {
                return true;
            }

            let mut install = std::process::Command::new("brew");
            install
                .args(["install", "postgresql@15"])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            return Self::run_command_success(&mut install);
        }

        #[cfg(target_os = "linux")]
        {
            let mut apt_direct = std::process::Command::new("apt-get");
            apt_direct
                .args(["update"])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if Self::run_command_success(&mut apt_direct) {
                let mut apt_install = std::process::Command::new("apt-get");
                apt_install
                    .args(["install", "-y", "postgresql-15"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if Self::run_command_success(&mut apt_install) {
                    return true;
                }
            }

            let mut sudo_update = std::process::Command::new("sudo");
            sudo_update
                .args(["-n", "apt-get", "update"])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if Self::run_command_success(&mut sudo_update) {
                let mut sudo_install = std::process::Command::new("sudo");
                sudo_install
                    .args(["-n", "apt-get", "install", "-y", "postgresql-15"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                return Self::run_command_success(&mut sudo_install);
            }
            return false;
        }

        #[cfg(target_os = "windows")]
        {
            let mut winget = std::process::Command::new("winget");
            winget
                .args([
                    "install",
                    "-e",
                    "--id",
                    "PostgreSQL.PostgreSQL.15",
                    "--accept-package-agreements",
                    "--accept-source-agreements",
                    "--silent",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            return Self::run_command_success(&mut winget);
        }

        #[allow(unreachable_code)]
        false
    }

    fn find_in_path(bin_name: &str) -> Option<PathBuf> {
        let path_var = std::env::var_os("PATH")?;
        std::env::split_paths(&path_var)
            .map(|dir| dir.join(bin_name))
            .find(|candidate| Self::is_runnable_binary(candidate))
    }

    fn initdb_if_needed(&self, settings: &SelfHostedSettings, data_dir: &Path) -> Result<()> {
        if data_dir.join("PG_VERSION").exists() {
            return Ok(());
        }

        if !data_dir.exists() {
            std::fs::create_dir_all(data_dir)?;
        }

        let initdb_bin = Self::postgres_bin_path("initdb")?;
        let mut pwfile = std::env::temp_dir();
        pwfile.push(format!(
            "kaspa-ng-pwfile-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::write(&pwfile, settings.db_password.as_bytes())?;

        let status = {
            let mut cmd = std::process::Command::new(initdb_bin);
            Self::apply_no_window_for_std_command(&mut cmd);
            cmd.arg("-D")
                .arg(data_dir)
                .arg("-U")
                .arg(&settings.db_user)
                .arg("--auth=md5")
                .arg("--encoding=UTF8")
                .arg(format!("--pwfile={}", pwfile.display()))
                .env("LC_MESSAGES", "C")
                .status()
        };

        let _ = std::fs::remove_file(&pwfile);

        match status {
            Ok(status) if status.success() => {
                Self::write_password_marker(data_dir, settings.db_password.as_str());
                Ok(())
            }
            Ok(status) => Err(Error::Custom(format!("initdb failed with status {status}"))),
            Err(err) => Err(Error::Custom(format!("initdb failed: {err}"))),
        }
    }

    fn admin_user_candidates() -> Vec<String> {
        let mut users = Vec::new();
        if let Ok(user) = std::env::var("USER") {
            if !user.trim().is_empty() {
                users.push(user);
            }
        }
        users.push("postgres".to_string());
        users
    }

    fn build_conn_str(
        host: &str,
        port: u16,
        user: &str,
        password: Option<&str>,
        dbname: &str,
    ) -> String {
        if let Some(password) = password {
            format!(
                "host={} port={} user={} password={} dbname={} connect_timeout=2",
                host, port, user, password, dbname
            )
        } else {
            format!(
                "host={} port={} user={} dbname={} connect_timeout=2",
                host, port, user, dbname
            )
        }
    }

    async fn wait_for_ready(
        settings: &SelfHostedSettings,
        node: &NodeSettings,
        retries: usize,
    ) -> Result<()> {
        let host = settings.db_host.clone();
        let port = settings.effective_db_port(node.network);
        let admin_users = Self::admin_user_candidates();
        let fallback_admin = admin_users.first().cloned();

        for _ in 0..retries {
            let conn_str = Self::build_conn_str(
                &host,
                port,
                &settings.db_user,
                Some(&settings.db_password),
                "postgres",
            );
            if let Ok((_, connection)) =
                tokio_postgres::connect(&conn_str, tokio_postgres::NoTls).await
            {
                spawn(async move {
                    let _ = connection.await;
                    Ok(())
                });
                return Ok(());
            }

            if let Some(admin) = fallback_admin.as_ref() {
                let conn_str = Self::build_conn_str(&host, port, admin, None, "postgres");
                if let Ok((_, connection)) =
                    tokio_postgres::connect(&conn_str, tokio_postgres::NoTls).await
                {
                    spawn(async move {
                        let _ = connection.await;
                        Ok(())
                    });
                    return Ok(());
                }
            }

            task::sleep(Duration::from_secs(2)).await;
        }

        Err(Error::Custom("postgres not ready".to_string()))
    }

    async fn can_connect_with_configured_credentials(
        settings: &SelfHostedSettings,
        node: &NodeSettings,
    ) -> bool {
        Self::can_connect_with_password(settings, node, settings.db_password.as_str()).await
    }

    async fn can_connect_with_password(
        settings: &SelfHostedSettings,
        node: &NodeSettings,
        password: &str,
    ) -> bool {
        let conn_str = Self::build_conn_str(
            &settings.db_host,
            settings.effective_db_port(node.network),
            &settings.db_user,
            Some(password),
            "postgres",
        );
        if let Ok((_, connection)) = tokio_postgres::connect(&conn_str, tokio_postgres::NoTls).await
        {
            spawn(async move {
                let _ = connection.await;
                Ok(())
            });
            return true;
        }
        false
    }

    fn read_primary_settings_password(network: Network) -> Option<String> {
        let home = workflow_core::dirs::home_dir()?;
        let path = home
            .join(".kaspa")
            .join(crate::settings::network_settings_filename(network));
        let content = std::fs::read_to_string(path).ok()?;
        let json = serde_json::from_str::<serde_json::Value>(&content).ok()?;
        json.get("self-hosted")
            .and_then(|v| v.get("db-password"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned)
    }

    async fn align_password_with_running_cluster(
        &self,
        settings: &mut SelfHostedSettings,
        node: &NodeSettings,
        data_dir: &Path,
    ) -> bool {
        if Self::can_connect_with_configured_credentials(settings, node).await {
            return true;
        }

        let mut candidates = Vec::<String>::new();
        if let Ok(marker) = std::fs::read_to_string(Self::password_marker_path(data_dir)) {
            let marker = marker.trim();
            if !marker.is_empty() && marker != settings.db_password {
                candidates.push(marker.to_string());
            }
        }
        if let Some(primary) = Self::read_primary_settings_password(node.network)
            && primary != settings.db_password
            && !candidates.iter().any(|c| c == &primary)
        {
            candidates.push(primary);
        }

        for candidate in candidates {
            if Self::can_connect_with_password(settings, node, candidate.as_str()).await {
                let msg = "adopted working postgres password from known source";
                self.logs.push("WARN", msg);
                settings.db_password = candidate.clone();
                if let Ok(mut guard) = self.settings.lock() {
                    guard.db_password = candidate.clone();
                }
                Self::write_password_marker(data_dir, candidate.as_str());
                return true;
            }
        }

        false
    }

    fn escape_literal(value: &str) -> String {
        value.replace('\'', "''")
    }

    fn recover_from_auth_mismatch(
        &self,
        settings: &SelfHostedSettings,
        data_dir: &Path,
    ) -> Result<bool> {
        let lock_path = Self::auth_recovery_lock_path(data_dir);
        if lock_path.exists() {
            self.logs.push(
                "ERROR",
                "auth mismatch recovery already attempted for this instance; manual reset required",
            );
            return Ok(false);
        }

        std::fs::write(&lock_path, b"1").map_err(|err| Error::Custom(err.to_string()))?;

        if data_dir.exists() {
            let backup = data_dir.with_extension(format!(
                "auth-mismatch-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            ));
            if let Err(err) = std::fs::rename(data_dir, &backup) {
                let _ = std::fs::remove_file(&lock_path);
                return Err(Error::Custom(format!(
                    "failed to backup postgres data dir '{}': {}",
                    data_dir.display(),
                    err
                )));
            }
            self.logs.push(
                "WARN",
                &format!(
                    "postgres auth mismatch detected; moved cluster to '{}' and reinitializing",
                    backup.display()
                ),
            );
        }

        std::fs::create_dir_all(data_dir).map_err(|err| Error::Custom(err.to_string()))?;
        self.initdb_if_needed(settings, data_dir)?;
        Self::write_password_marker(data_dir, settings.db_password.as_str());
        Ok(true)
    }

    fn stop_external_cluster(data_dir: &Path) {
        #[cfg(not(unix))]
        let _ = data_dir;

        #[cfg(unix)]
        {
            let pid_file = data_dir.join("postmaster.pid");
            if let Ok(content) = std::fs::read_to_string(pid_file)
                && let Some(first) = content.lines().next()
                && let Ok(pid_i32) = first.trim().parse::<i32>()
            {
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;
                let _ = kill(Pid::from_raw(pid_i32), Signal::SIGTERM);
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    }

    async fn ensure_role_and_database(
        settings: &SelfHostedSettings,
        node: &NodeSettings,
        logs: &LogStore,
    ) {
        let psql_bin = match Self::postgres_bin_path("psql") {
            Ok(path) => path,
            Err(err) => {
                log_warn!("self-hosted-postgres: {err}");
                logs.push("WARN", &format!("{err}"));
                return;
            }
        };

        let db_user = settings.db_user.trim();
        let db_password = settings.db_password.trim();
        let db_name = crate::settings::self_hosted_db_name_for_network(
            settings.db_name.as_str(),
            node.network,
        );
        let primary_db_port = settings.effective_db_port(node.network);
        let mut db_ports = vec![primary_db_port];
        {
            let network = Network::Mainnet;
            let port = settings.effective_db_port(network);
            if !db_ports.contains(&port) {
                db_ports.push(port);
            }
        }
        let db_name = db_name.trim();

        if db_user.is_empty() || db_name.is_empty() {
            return;
        }

        let role_exists_sql = format!(
            "SELECT 1 FROM pg_roles WHERE rolname = '{}';",
            Self::escape_literal(db_user),
        );
        let create_role_sql = format!(
            "CREATE ROLE \"{}\" LOGIN PASSWORD '{}';",
            db_user.replace('\"', "\"\""),
            Self::escape_literal(db_password),
        );
        let db_exists_sql = format!(
            "SELECT 1 FROM pg_database WHERE datname='{}';",
            Self::escape_literal(db_name),
        );
        let create_db_sql = format!(
            "CREATE DATABASE \"{}\" OWNER \"{}\";",
            db_name.replace('\"', "\"\""),
            db_user.replace('\"', "\"\""),
        );

        let run_psql = |db_port: u16,
                        admin: Option<&str>,
                        host: Option<&str>,
                        use_tcp: bool,
                        sql: &str|
         -> Result<String> {
            let mut cmd = std::process::Command::new(&psql_bin);
            Self::apply_no_window_for_std_command(&mut cmd);
            cmd.arg("-X")
                .arg("-v")
                .arg("ON_ERROR_STOP=1")
                .arg("-w")
                .arg("-d")
                .arg("postgres")
                .arg("-tAc")
                .arg(sql);
            cmd.arg("-p").arg(db_port.to_string());
            match (use_tcp, host) {
                (true, Some(host)) => {
                    cmd.arg("-h").arg(host);
                }
                (true, None) => {
                    cmd.arg("-h").arg(settings.db_host.clone());
                }
                (false, Some(host)) => {
                    cmd.arg("-h").arg(host);
                }
                (false, None) => {}
            }
            if let Some(admin) = admin {
                cmd.arg("-U").arg(admin);
            }
            cmd.env("PGPASSWORD", db_password);
            cmd.env("LC_MESSAGES", "C");
            let output = cmd.output().map_err(|err| Error::Custom(err.to_string()))?;
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                return Ok(stdout.trim().to_string());
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(Error::Custom(stderr.trim().to_string()))
        };

        let mut last_error: Option<String> = None;

        let try_psql = |db_port: u16,
                        admin: Option<&str>,
                        host: Option<&str>,
                        use_tcp: bool,
                        last_error: &mut Option<String>| {
            let role_exists = match run_psql(db_port, admin, host, use_tcp, &role_exists_sql) {
                Ok(out) => !out.is_empty(),
                Err(err) => {
                    *last_error = Some(format!(
                        "role check failed (port={}, admin={:?}, host={:?}, tcp={}): {}",
                        db_port, admin, host, use_tcp, err
                    ));
                    return false;
                }
            };

            if !role_exists {
                if let Err(err) = run_psql(db_port, admin, host, use_tcp, &create_role_sql) {
                    *last_error = Some(format!(
                        "role creation failed (port={}, admin={:?}, host={:?}, tcp={}): {}",
                        db_port, admin, host, use_tcp, err
                    ));
                    return false;
                }
            }

            let db_exists = match run_psql(db_port, admin, host, use_tcp, &db_exists_sql) {
                Ok(out) => !out.is_empty(),
                Err(err) => {
                    *last_error = Some(format!(
                        "db check failed (port={}, admin={:?}, host={:?}, tcp={}): {}",
                        db_port, admin, host, use_tcp, err
                    ));
                    return false;
                }
            };

            if db_exists {
                return true;
            }

            if let Err(err) = run_psql(db_port, admin, host, use_tcp, &create_db_sql) {
                *last_error = Some(format!(
                    "db creation failed (port={}, admin={:?}, host={:?}, tcp={}): {}",
                    db_port, admin, host, use_tcp, err
                ));
                return false;
            }

            true
        };

        #[cfg(unix)]
        let socket_hosts = vec![
            "/tmp",
            "/private/tmp",
            "/var/run/postgresql",
            "/run/postgresql",
        ];
        #[cfg(not(unix))]
        let socket_hosts: Vec<&str> = Vec::new();

        for _ in 0..5 {
            for db_port in &db_ports {
                if !db_user.is_empty() {
                    if try_psql(*db_port, Some(db_user), None, false, &mut last_error) {
                        return;
                    }
                    for socket in &socket_hosts {
                        if try_psql(
                            *db_port,
                            Some(db_user),
                            Some(socket),
                            false,
                            &mut last_error,
                        ) {
                            return;
                        }
                    }
                    if try_psql(*db_port, Some(db_user), None, true, &mut last_error) {
                        return;
                    }
                }

                let admin_users = Self::admin_user_candidates();
                for admin in admin_users {
                    if try_psql(*db_port, Some(&admin), None, false, &mut last_error) {
                        return;
                    }
                    for socket in &socket_hosts {
                        if try_psql(*db_port, Some(&admin), Some(socket), false, &mut last_error) {
                            return;
                        }
                    }
                    if try_psql(*db_port, Some(&admin), None, true, &mut last_error) {
                        return;
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        log_warn!(
            "self-hosted-postgres: unable to create role/database; ensure a superuser is available"
        );
        logs.push(
            "WARN",
            "unable to create role/database; ensure a superuser is available",
        );
        if let Some(err) = last_error {
            log_warn!("self-hosted-postgres: last error: {}", err);
            logs.push("WARN", &format!("last error: {err}"));
        }
    }

    async fn reset_all_network_databases(
        settings: &SelfHostedSettings,
        node: &NodeSettings,
        logs: &LogStore,
    ) {
        let psql_bin = match Self::postgres_bin_path("psql") {
            Ok(path) => path,
            Err(err) => {
                log_warn!("self-hosted-postgres: {err}");
                logs.push("WARN", &format!("{err}"));
                return;
            }
        };

        let db_user = settings.db_user.trim();
        let db_password = settings.db_password.trim();
        let db_port = settings.effective_db_port(node.network);
        if db_user.is_empty() {
            logs.push("WARN", "database user is empty; cannot reset databases");
            return;
        }

        let db_names = Self::all_network_db_names(settings);
        logs.push(
            "INFO",
            &format!(
                "reset requested for self-hosted databases: {}",
                db_names.join(", ")
            ),
        );

        let role_exists_sql = format!(
            "SELECT 1 FROM pg_roles WHERE rolname = '{}';",
            Self::escape_literal(db_user),
        );
        let create_role_sql = format!(
            "CREATE ROLE \"{}\" LOGIN PASSWORD '{}';",
            db_user.replace('\"', "\"\""),
            Self::escape_literal(db_password),
        );

        let run_psql =
            |admin: Option<&str>, host: Option<&str>, use_tcp: bool, sql: &str| -> Result<String> {
                let mut cmd = std::process::Command::new(&psql_bin);
                Self::apply_no_window_for_std_command(&mut cmd);
                cmd.arg("-X")
                    .arg("-v")
                    .arg("ON_ERROR_STOP=1")
                    .arg("-w")
                    .arg("-d")
                    .arg("postgres")
                    .arg("-tAc")
                    .arg(sql);
                cmd.arg("-p").arg(db_port.to_string());
                match (use_tcp, host) {
                    (true, Some(host)) => {
                        cmd.arg("-h").arg(host);
                    }
                    (true, None) => {
                        cmd.arg("-h").arg(settings.db_host.clone());
                    }
                    (false, Some(host)) => {
                        cmd.arg("-h").arg(host);
                    }
                    (false, None) => {}
                }
                if let Some(admin) = admin {
                    cmd.arg("-U").arg(admin);
                }
                cmd.env("PGPASSWORD", db_password);
                cmd.env("LC_MESSAGES", "C");
                let output = cmd.output().map_err(|err| Error::Custom(err.to_string()))?;
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    return Ok(stdout.trim().to_string());
                }
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(Error::Custom(stderr.trim().to_string()))
            };

        let mut last_error: Option<String> = None;
        let try_psql = |admin: Option<&str>,
                        host: Option<&str>,
                        use_tcp: bool,
                        last_error: &mut Option<String>|
         -> bool {
            let role_exists = match run_psql(admin, host, use_tcp, &role_exists_sql) {
                Ok(out) => !out.is_empty(),
                Err(err) => {
                    *last_error = Some(format!(
                        "role check failed (admin={:?}, host={:?}, tcp={}): {}",
                        admin, host, use_tcp, err
                    ));
                    return false;
                }
            };

            if !role_exists && let Err(err) = run_psql(admin, host, use_tcp, &create_role_sql) {
                *last_error = Some(format!(
                    "role creation failed (admin={:?}, host={:?}, tcp={}): {}",
                    admin, host, use_tcp, err
                ));
                return false;
            }

            for db_name in &db_names {
                let drop_db_sql = format!(
                    "DROP DATABASE IF EXISTS \"{}\" WITH (FORCE);",
                    db_name.replace('\"', "\"\""),
                );
                if let Err(err) = run_psql(admin, host, use_tcp, &drop_db_sql) {
                    *last_error = Some(format!(
                        "db drop failed for '{}' (admin={:?}, host={:?}, tcp={}): {}",
                        db_name, admin, host, use_tcp, err
                    ));
                    return false;
                }

                let create_db_sql = format!(
                    "CREATE DATABASE \"{}\" OWNER \"{}\";",
                    db_name.replace('\"', "\"\""),
                    db_user.replace('\"', "\"\""),
                );
                if let Err(err) = run_psql(admin, host, use_tcp, &create_db_sql) {
                    *last_error = Some(format!(
                        "db create failed for '{}' (admin={:?}, host={:?}, tcp={}): {}",
                        db_name, admin, host, use_tcp, err
                    ));
                    return false;
                }
            }

            true
        };

        let host_candidates = [Some(settings.db_host.as_str()), Some("127.0.0.1"), None];
        let socket_hosts = ["/tmp", "/var/run/postgresql"];

        for _ in 0..10 {
            for host in &host_candidates {
                if try_psql(Some(db_user), *host, true, &mut last_error)
                    || try_psql(None, *host, true, &mut last_error)
                {
                    logs.push(
                        "INFO",
                        &format!("self-hosted databases reset: {}", db_names.join(", ")),
                    );
                    return;
                }
            }

            for socket in &socket_hosts {
                if try_psql(Some(db_user), Some(socket), false, &mut last_error)
                    || try_psql(None, Some(socket), false, &mut last_error)
                {
                    logs.push(
                        "INFO",
                        &format!("self-hosted databases reset: {}", db_names.join(", ")),
                    );
                    return;
                }
            }

            let admin_users = Self::admin_user_candidates();
            for admin in admin_users {
                if try_psql(Some(&admin), None, false, &mut last_error) {
                    logs.push(
                        "INFO",
                        &format!("self-hosted databases reset: {}", db_names.join(", ")),
                    );
                    return;
                }
                for socket in &socket_hosts {
                    if try_psql(Some(&admin), Some(socket), false, &mut last_error) {
                        logs.push(
                            "INFO",
                            &format!("self-hosted databases reset: {}", db_names.join(", ")),
                        );
                        return;
                    }
                }
                if try_psql(Some(&admin), None, true, &mut last_error) {
                    logs.push(
                        "INFO",
                        &format!("self-hosted databases reset: {}", db_names.join(", ")),
                    );
                    return;
                }
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        log_warn!(
            "self-hosted-postgres: unable to reset self-hosted databases; ensure a superuser is available"
        );
        logs.push(
            "WARN",
            "unable to reset self-hosted databases; ensure a superuser is available",
        );
        if let Some(err) = last_error {
            logs.push("WARN", &format!("last error: {err}"));
        }
    }

    async fn start_postgres(self: &Arc<Self>) -> Result<()> {
        let mut settings = self.settings.lock().unwrap().clone();
        let node_settings = self.node_settings.lock().unwrap().clone();
        let db_port = settings.effective_db_port(node_settings.network);
        if !settings.enabled || !settings.postgres_enabled {
            return Ok(());
        }

        let data_dir = Self::resolve_data_dir(&settings, node_settings.network)?;
        let password_marker = Self::password_marker_path(&data_dir);
        let _ = self
            .align_password_with_running_cluster(&mut settings, &node_settings, &data_dir)
            .await;
        let postmaster_pid = data_dir.join("postmaster.pid");
        if postmaster_pid.exists() {
            if Self::wait_for_ready(&settings, &node_settings, 5)
                .await
                .is_ok()
            {
                let can_auth =
                    Self::can_connect_with_configured_credentials(&settings, &node_settings).await;
                if can_auth {
                    if !password_marker.exists() {
                        Self::write_password_marker(&data_dir, settings.db_password.as_str());
                    }
                    let msg = "postmaster.pid exists; assuming postgres is already running";
                    log_info!("self-hosted-postgres: {msg}");
                    self.logs.push("INFO", msg);
                    self.logs.push(
                        "INFO",
                        "external postgres detected; log streaming only available when started by Kaspa NG",
                    );
                    Self::ensure_role_and_database(&settings, &node_settings, &self.logs).await;
                    return Ok(());
                }

                self.logs.push(
                    "ERROR",
                    "detected running postgres with credential mismatch; stopping stale cluster and reinitializing",
                );
                Self::stop_external_cluster(&data_dir);
                let _ = std::fs::remove_file(&postmaster_pid);
                if self.recover_from_auth_mismatch(&settings, &data_dir)? {
                    self.logs.push(
                        "INFO",
                        "postgres auth recovery finished; starting recovered cluster",
                    );
                }
            }

            let msg = "postmaster.pid exists but postgres is not reachable; removing stale pid";
            log_info!("self-hosted-postgres: {msg}");
            self.logs.push("INFO", msg);
            let _ = std::fs::remove_file(&postmaster_pid);
        }
        self.initdb_if_needed(&settings, &data_dir)?;

        let postgres_bin = Self::postgres_bin_path("postgres")?;
        let mut cmd = Command::new(postgres_bin);
        cmd.arg("-D")
            .arg(&data_dir)
            .arg("-p")
            .arg(db_port.to_string())
            .arg("-h")
            .arg(settings.db_host.clone())
            .arg("-c")
            .arg("max_wal_size=4GB")
            .arg("-c")
            .arg("checkpoint_timeout=5min")
            .arg("-c")
            .arg("checkpoint_completion_target=0.9")
            .arg("-c")
            .arg("lc_messages=C")
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("LC_MESSAGES", "C")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

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
                log_warn!("self-hosted-postgres: failed to start ({err})");
                self.logs.push("WARN", &format!("failed to start ({err})"));
                return Err(err);
            }
        };

        let logs_info = self.logs.clone();
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let level = Self::detect_log_level(&line, "INFO");
                    if level == "IGNORE" {
                        continue;
                    }
                    match level {
                        "ERROR" => log_warn!("self-hosted-postgres: {line}"),
                        "WARN" => log_warn!("self-hosted-postgres: {line}"),
                        _ => log_info!("self-hosted-postgres: {line}"),
                    }
                    logs_info.push(level, &line);
                }
            });
        }

        let logs_warn = self.logs.clone();
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let level = Self::detect_log_level(&line, "WARN");
                    if level == "IGNORE" {
                        continue;
                    }
                    match level {
                        "ERROR" => log_warn!("self-hosted-postgres: {line}"),
                        "WARN" => log_warn!("self-hosted-postgres: {line}"),
                        _ => log_info!("self-hosted-postgres: {line}"),
                    }
                    logs_warn.push(level, &line);
                }
            });
        }

        *self.child.lock().unwrap() = Some(child);
        *self.last_restart_at.lock().unwrap() = Some(Instant::now());
        if Self::wait_for_ready(&settings, &node_settings, 20)
            .await
            .is_ok()
        {
            let mut can_auth =
                Self::can_connect_with_configured_credentials(&settings, &node_settings).await;
            if !can_auth {
                self.logs.push(
                    "WARN",
                    "configured postgres credentials not ready after startup; attempting role/password alignment",
                );
                Self::ensure_role_and_database(&settings, &node_settings, &self.logs).await;
                for _ in 0..4 {
                    if Self::can_connect_with_configured_credentials(&settings, &node_settings)
                        .await
                    {
                        can_auth = true;
                        break;
                    }
                    task::sleep(Duration::from_secs(1)).await;
                }

                if !can_auth {
                    self.logs.push(
                        "ERROR",
                        "password authentication mismatch detected for local postgres; attempting one-time cluster recovery",
                    );
                    log_warn!(
                        "self-hosted-postgres: triggering local postgres stop for auth mismatch recovery"
                    );
                    let _ = self.stop_postgres().await;
                    if self.recover_from_auth_mismatch(&settings, &data_dir)? {
                        self.logs.push(
                            "INFO",
                            "postgres auth recovery finished; service will restart postgres on next tick",
                        );
                        return Ok(());
                    }
                    return Ok(());
                }
            }

            let recovery_lock = Self::auth_recovery_lock_path(&data_dir);
            if recovery_lock.exists() {
                let _ = std::fs::remove_file(recovery_lock);
            }
            if !Self::marker_matches_password(&data_dir, settings.db_password.as_str()) {
                Self::write_password_marker(&data_dir, settings.db_password.as_str());
            }
            Self::ensure_role_and_database(&settings, &node_settings, &self.logs).await;
        }
        Ok(())
    }

    async fn stop_postgres(&self) -> Result<()> {
        let child = self.child.lock().unwrap().take();
        if let Some(mut child) = child {
            let mut exited = false;

            #[cfg(unix)]
            {
                if let Some(pid_u32) = child.id() {
                    use nix::sys::signal::{Signal, kill};
                    use nix::unistd::Pid;

                    if let Ok(pid_i32) = i32::try_from(pid_u32) {
                        let _ = kill(Pid::from_raw(pid_i32), Signal::SIGTERM);
                        self.logs
                            .push("INFO", "sent SIGTERM to postgres for graceful shutdown");
                    }
                }
            }

            if tokio::time::timeout(Duration::from_secs(25), child.wait())
                .await
                .is_ok()
            {
                exited = true;
            }

            if !exited {
                self.logs.push(
                    "WARN",
                    "postgres did not exit after SIGTERM; retrying graceful shutdown",
                );
                #[cfg(unix)]
                {
                    if let Some(pid_u32) = child.id() {
                        use nix::sys::signal::{Signal, kill};
                        use nix::unistd::Pid;
                        if let Ok(pid_i32) = i32::try_from(pid_u32) {
                            let _ = kill(Pid::from_raw(pid_i32), Signal::SIGINT);
                        }
                    }
                }

                if tokio::time::timeout(Duration::from_secs(10), child.wait())
                    .await
                    .is_err()
                {
                    self.logs
                        .push("WARN", "postgres did not exit in time; forcing termination");
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Service for SelfHostedPostgresService {
    fn name(&self) -> &'static str {
        "self-hosted-postgres"
    }

    async fn spawn(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        tokio::spawn(async move {
            if this.is_enabled.load(Ordering::SeqCst) {
                let _ = this.start_postgres().await;
            }

            loop {
                select! {
                    msg = this.service_events.receiver.recv().fuse() => {
                        match msg {
                            Ok(SelfHostedPostgresEvents::Enable) => {
                                let was_enabled = this.is_enabled.swap(true, Ordering::SeqCst);
                                if !was_enabled {
                                    this.arm_startup_restart_guard();
                                    let _ = this.start_postgres().await;
                                }
                            }
                            Ok(SelfHostedPostgresEvents::Disable) => {
                                let was_enabled = this.is_enabled.swap(false, Ordering::SeqCst);
                                if was_enabled {
                                    this.clear_startup_restart_guard();
                                    let _ = this.stop_postgres().await;
                                }
                            }
                            Ok(SelfHostedPostgresEvents::UpdateSettings(settings)) => {
                                let previous_settings = this.settings.lock().unwrap().clone();
                                *this.settings.lock().unwrap() = settings.clone();
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let node_settings = this.node_settings.lock().unwrap().clone();
                                    if Self::requires_restart_for_settings_change(
                                        &previous_settings,
                                        &settings,
                                        node_settings.network,
                                    ) {
                                        let previous_port =
                                            previous_settings.effective_db_port(node_settings.network);
                                        let next_port = settings.effective_db_port(node_settings.network);
                                        if this.should_skip_restart_due_to_startup_guard(
                                            previous_port,
                                            next_port,
                                            "settings change",
                                        ) {
                                            continue;
                                        }
                                        if this.should_skip_restart_due_to_cooldown(
                                            previous_port,
                                            next_port,
                                            "settings change",
                                        ) {
                                            continue;
                                        }
                                        this.logs.push(
                                            "INFO",
                                            &format!(
                                                "postgres settings change requires restart (port {} -> {})",
                                                previous_port, next_port
                                            ),
                                        );
                                        let _ = this.stop_postgres().await;
                                        let _ = this.start_postgres().await;
                                    } else if Self::wait_for_ready(&settings, &node_settings, 20)
                                        .await
                                        .is_ok()
                                    {
                                        Self::ensure_role_and_database(
                                            &settings,
                                            &node_settings,
                                            &this.logs,
                                        )
                                        .await;
                                    } else {
                                        this.logs.push(
                                            "INFO",
                                            "postgres is restarting; role/database check deferred",
                                        );
                                    }
                                }
                            }
                            Ok(SelfHostedPostgresEvents::UpdateNodeSettings(settings)) => {
                                *this.node_settings.lock().unwrap() = settings;
                                // Keep node context updated, but do not block this event loop here.
                                // Doing wait_for_ready() here can stall Disable/Enable events by ~40s.
                            }
                            Ok(SelfHostedPostgresEvents::ResetDatabases) => {
                                let settings = this.settings.lock().unwrap().clone();
                                let node_settings = this.node_settings.lock().unwrap().clone();
                                if !settings.enabled || !settings.postgres_enabled {
                                    this.logs.push("WARN", "self-hosted postgres is disabled; reset skipped");
                                    continue;
                                }
                                if Self::wait_for_ready(&settings, &node_settings, 20)
                                    .await
                                    .is_err()
                                {
                                    this.logs.push("WARN", "postgres is not ready; reset skipped");
                                    continue;
                                }
                                Self::reset_all_network_databases(
                                    &settings,
                                    &node_settings,
                                    &this.logs,
                                )
                                .await;
                            }
                            Ok(SelfHostedPostgresEvents::Exit) | Err(_) => {
                                let _ = this.stop_postgres().await;
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
            .try_send(SelfHostedPostgresEvents::Exit);
    }

    async fn join(self: Arc<Self>) -> Result<()> {
        self.task_ctl.recv().await.unwrap();
        Ok(())
    }
}
