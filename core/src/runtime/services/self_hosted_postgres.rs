use crate::imports::*;
use crate::runtime::services::{LogStore, LogStores};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

pub enum SelfHostedPostgresEvents {
    Enable,
    Disable,
    UpdateSettings(SelfHostedSettings),
    Exit,
}

pub struct SelfHostedPostgresService {
    pub application_events: ApplicationEventsChannel,
    pub service_events: Channel<SelfHostedPostgresEvents>,
    pub task_ctl: Channel<()>,
    pub settings: Mutex<SelfHostedSettings>,
    pub is_enabled: AtomicBool,
    logs: Arc<LogStore>,
    child: Mutex<Option<Child>>,
}

impl SelfHostedPostgresService {
    fn detect_log_level<'a>(line: &str, fallback: &'a str) -> &'a str {
        let upper = line.to_ascii_uppercase();
        // During crash recovery Postgres emits transient connection errors while still booting.
        // Treat these as informational startup progress to avoid noisy false alarms.
        if upper.contains("DATABASE SYSTEM IS NOT YET ACCEPTING CONNECTIONS")
            || upper.contains("CONSISTENT RECOVERY STATE HAS NOT BEEN YET REACHED")
        {
            return "INFO";
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
            is_enabled: AtomicBool::new(
                settings.self_hosted.enabled && settings.self_hosted.postgres_enabled,
            ),
            logs: logs.postgres,
            child: Mutex::new(None),
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

    fn resolve_data_dir(settings: &SelfHostedSettings) -> Result<PathBuf> {
        if !settings.postgres_data_dir.trim().is_empty() {
            return Ok(PathBuf::from(settings.postgres_data_dir.trim()));
        }

        let default_storage_folder = kaspa_wallet_core::storage::local::default_storage_folder();
        let storage_folder = workflow_store::fs::resolve_path(default_storage_folder)?;
        Ok(storage_folder.join("self-hosted").join("postgres"))
    }

    fn postgres_bin_path(binary: &str) -> Result<PathBuf> {
        let bin_name = if cfg!(windows) {
            format!("{binary}.exe")
        } else {
            binary.to_string()
        };

        for bin_dir in Self::candidate_bin_dirs() {
            let candidate = PathBuf::from(bin_dir).join(&bin_name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }

        if let Some(path) = Self::find_in_path(&bin_name) {
            return Ok(path);
        }

        Err(Error::Custom(format!(
            "postgres binary not found: {bin_name} (searched common install paths and PATH)"
        )))
    }

    fn candidate_bin_dirs() -> Vec<&'static str> {
        let mut dirs = Vec::new();

        #[cfg(target_os = "macos")]
        {
            dirs.extend([
                "/opt/homebrew/opt/postgresql@15/bin",
                "/usr/local/opt/postgresql@15/bin",
                "/opt/homebrew/opt/postgresql/bin",
                "/usr/local/opt/postgresql/bin",
            ]);
        }

        #[cfg(target_os = "linux")]
        {
            dirs.extend([
                "/usr/lib/postgresql/15/bin",
                "/usr/pgsql-15/bin",
                "/usr/local/pgsql/bin",
                "/usr/local/bin",
                "/usr/bin",
            ]);
        }

        #[cfg(target_os = "windows")]
        {
            dirs.extend([
                "C:\\Program Files\\PostgreSQL\\15\\bin",
                "C:\\Program Files (x86)\\PostgreSQL\\15\\bin",
            ]);
        }

        dirs
    }

    fn find_in_path(bin_name: &str) -> Option<PathBuf> {
        let path_var = std::env::var_os("PATH")?;
        std::env::split_paths(&path_var)
            .map(|dir| dir.join(bin_name))
            .find(|candidate| candidate.exists())
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

        let status = std::process::Command::new(initdb_bin)
            .arg("-D")
            .arg(data_dir)
            .arg("-U")
            .arg(&settings.db_user)
            .arg("--auth=md5")
            .arg("--encoding=UTF8")
            .arg(format!("--pwfile={}", pwfile.display()))
            .env("LC_MESSAGES", "C")
            .status();

        let _ = std::fs::remove_file(&pwfile);

        match status {
            Ok(status) if status.success() => Ok(()),
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

    async fn wait_for_ready(settings: &SelfHostedSettings, retries: usize) -> Result<()> {
        let host = settings.db_host.clone();
        let port = settings.db_port;
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

            task::sleep(Duration::from_secs(1)).await;
        }

        Err(Error::Custom("postgres not ready".to_string()))
    }

    fn escape_literal(value: &str) -> String {
        value.replace('\'', "''")
    }

    async fn ensure_role_and_database(settings: &SelfHostedSettings, logs: &LogStore) {
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
        let db_name = settings.db_name.trim();

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

        let run_psql = |admin: Option<&str>,
                        host: Option<&str>,
                        use_tcp: bool,
                        sql: &str|
         -> Result<String> {
                let mut cmd = std::process::Command::new(&psql_bin);
                cmd.arg("-X")
                    .arg("-v")
                    .arg("ON_ERROR_STOP=1")
                    .arg("-w")
                    .arg("-d")
                    .arg("postgres")
                    .arg("-tAc")
                    .arg(sql);
                cmd.arg("-p").arg(settings.db_port.to_string());
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
                        last_error: &mut Option<String>| {
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

            if !role_exists {
                if let Err(err) = run_psql(admin, host, use_tcp, &create_role_sql) {
                    *last_error = Some(format!(
                        "role creation failed (admin={:?}, host={:?}, tcp={}): {}",
                        admin, host, use_tcp, err
                    ));
                    return false;
                }
            }

            let db_exists = match run_psql(admin, host, use_tcp, &db_exists_sql) {
                Ok(out) => !out.is_empty(),
                Err(err) => {
                    *last_error = Some(format!(
                        "db check failed (admin={:?}, host={:?}, tcp={}): {}",
                        admin, host, use_tcp, err
                    ));
                    return false;
                }
            };

            if db_exists {
                return true;
            }

            if let Err(err) = run_psql(admin, host, use_tcp, &create_db_sql) {
                *last_error = Some(format!(
                    "db creation failed (admin={:?}, host={:?}, tcp={}): {}",
                    admin, host, use_tcp, err
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
            if !db_user.is_empty() {
                if try_psql(Some(db_user), None, false, &mut last_error) {
                    return;
                }
                for socket in &socket_hosts {
                    if try_psql(Some(db_user), Some(socket), false, &mut last_error) {
                        return;
                    }
                }
                if try_psql(Some(db_user), None, true, &mut last_error) {
                    return;
                }
            }

            let admin_users = Self::admin_user_candidates();
            for admin in admin_users {
                if try_psql(Some(&admin), None, false, &mut last_error) {
                    return;
                }
                for socket in &socket_hosts {
                    if try_psql(Some(&admin), Some(socket), false, &mut last_error) {
                        return;
                    }
                }
                if try_psql(Some(&admin), None, true, &mut last_error) {
                    return;
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

    async fn start_postgres(self: &Arc<Self>) -> Result<()> {
        let settings = self.settings.lock().unwrap().clone();
        if !settings.enabled || !settings.postgres_enabled {
            return Ok(());
        }

        let data_dir = Self::resolve_data_dir(&settings)?;
        let postmaster_pid = data_dir.join("postmaster.pid");
        if postmaster_pid.exists() {
            if Self::wait_for_ready(&settings, 5).await.is_ok() {
                let msg = "postmaster.pid exists; assuming postgres is already running";
                log_info!("self-hosted-postgres: {msg}");
                self.logs.push("INFO", msg);
                self.logs.push(
                    "INFO",
                    "external postgres detected; log streaming only available when started by Kaspa NG",
                );
                Self::ensure_role_and_database(&settings, &self.logs).await;
                return Ok(());
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
            .arg(settings.db_port.to_string())
            .arg("-h")
            .arg(settings.db_host.clone())
            .arg("-c")
            .arg("max_wal_size=4GB")
            .arg("-c")
            .arg("checkpoint_timeout=15min")
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
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
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
        if Self::wait_for_ready(&settings, 20).await.is_ok() {
            Self::ensure_role_and_database(&settings, &self.logs).await;
        }
        Ok(())
    }

    async fn stop_postgres(&self) -> Result<()> {
        let child = self.child.lock().unwrap().take();
        if let Some(mut child) = child {
            let _ = child.start_kill();
            let _ = child.wait().await;
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
                                    let _ = this.start_postgres().await;
                                }
                            }
                            Ok(SelfHostedPostgresEvents::Disable) => {
                                let was_enabled = this.is_enabled.swap(false, Ordering::SeqCst);
                                if was_enabled {
                                    let _ = this.stop_postgres().await;
                                }
                            }
                            Ok(SelfHostedPostgresEvents::UpdateSettings(settings)) => {
                                *this.settings.lock().unwrap() = settings;
                                if this.is_enabled.load(Ordering::SeqCst) {
                                    let _ = this.stop_postgres().await;
                                    let _ = this.start_postgres().await;
                                }
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
