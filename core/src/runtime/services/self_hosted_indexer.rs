use crate::imports::*;
use crate::runtime::services::{LogStore, LogStores};
use std::net::TcpListener;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

pub enum SelfHostedIndexerEvents {
    Enable,
    Disable,
    UpdateSettings(SelfHostedSettings),
    Exit,
}

pub struct SelfHostedIndexerService {
    pub application_events: ApplicationEventsChannel,
    pub service_events: Channel<SelfHostedIndexerEvents>,
    pub task_ctl: Channel<()>,
    pub settings: Mutex<SelfHostedSettings>,
    pub is_enabled: AtomicBool,
    logs: Arc<LogStore>,
    child: Mutex<Option<Child>>,
}

impl SelfHostedIndexerService {
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
            is_enabled: AtomicBool::new(
                settings.self_hosted.enabled && settings.self_hosted.indexer_enabled,
            ),
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

    fn build_database_url(settings: &SelfHostedSettings) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            settings.db_user,
            settings.db_password,
            settings.db_host,
            settings.db_port,
            settings.db_name
        )
    }

    async fn wait_for_database(settings: &SelfHostedSettings) -> Result<()> {
        let mut last_error: Option<String> = None;
        for attempt in 0..20 {
            let admin_conn_str = format!(
                "host={} port={} user={} password={} dbname=postgres connect_timeout=3",
                settings.db_host, settings.db_port, settings.db_user, settings.db_password
            );
            match tokio_postgres::connect(&admin_conn_str, tokio_postgres::NoTls).await {
                Ok((admin_client, admin_connection)) => {
                    spawn(async move {
                        let _ = admin_connection.await;
                        Ok(())
                    });

                    let exists = admin_client
                        .query_opt(
                            "SELECT 1 FROM pg_database WHERE datname = $1",
                            &[&settings.db_name],
                        )
                        .await
                        .map(|row| row.is_some())
                        .unwrap_or(false);

                    if exists {
                        let conn_str = format!(
                            "host={} port={} user={} password={} dbname={} connect_timeout=3",
                            settings.db_host,
                            settings.db_port,
                            settings.db_user,
                            settings.db_password,
                            settings.db_name
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
                            Some(format!("database '{}' is not ready yet", settings.db_name));
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
        if !settings.indexer_binary.trim().is_empty() {
            let custom = PathBuf::from(settings.indexer_binary.trim());
            if custom.exists() {
                return Some(custom);
            }
        }

        let rel_candidates = [
            "simply-kaspa-indexer/target/release/simply-kaspa-indexer",
            "simply-kaspa-indexer/target/debug/simply-kaspa-indexer",
        ];

        for candidate in rel_candidates {
            let path = PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for candidate in rel_candidates {
                    let path = dir.join(candidate);
                    if path.exists() {
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
        if !settings.enabled || !settings.indexer_enabled {
            return Ok(());
        }

        if !Self::listen_addr_available(&settings.indexer_listen) {
            log_warn!(
                "self-hosted-indexer: listen address already in use ({}); refusing to start indexer",
                settings.indexer_listen
            );
            self.logs.push(
                "ERROR",
                &format!(
                    "listen address already in use ({}); refusing to start indexer",
                    settings.indexer_listen
                ),
            );
            return Ok(());
        }

        if let Err(err) = Self::wait_for_database(&settings).await {
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

        let database_url = Self::build_database_url(&settings);

        let mut cmd = Command::new(binary);
        cmd.arg("-s")
            .arg(settings.indexer_rpc_url)
            .arg("-d")
            .arg(database_url)
            .arg("-l")
            .arg(settings.indexer_listen);

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

        *self.child.lock().unwrap() = Some(child);
        Ok(())
    }

    async fn stop_indexer(&self) -> Result<()> {
        let child = self.child.lock().unwrap().take();
        if let Some(mut child) = child {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
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
