use crate::imports::*;
use crate::runtime::Service;

cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        use crate::runtime::services::kaspa::logs::Log;
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::{Child, Command};

        const LOG_BUFFER_LINES: usize = 4096;
        const LOG_BUFFER_MARGIN: usize = 128;
        const RESTART_DELAY: Duration = Duration::from_secs(3);

        pub fn update_logs_flag() -> &'static Arc<AtomicBool> {
            static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();
            FLAG.get_or_init(|| Arc::new(AtomicBool::new(false)))
        }

        fn is_supported_network(network: Network) -> bool {
            matches!(network, Network::Testnet10 | Network::Testnet12)
        }

        #[derive(Debug, Clone)]
        enum RothschildEvents {
            SetEnabled {
                enabled: bool,
                network: Network,
                settings: RothschildSettings,
            },
            UpdateSettings {
                network: Network,
                settings: RothschildSettings,
            },
            Exit,
        }

        pub struct RothschildService {
            application_events: ApplicationEventsChannel,
            service_events: Channel<RothschildEvents>,
            task_ctl: Channel<()>,
            is_enabled: AtomicBool,
            starting: AtomicBool,
            restart_pending: AtomicBool,
            logs: Mutex<Vec<Log>>,
            network: Mutex<Network>,
            settings: Mutex<RothschildSettings>,
            child: Mutex<Option<Child>>,
        }

        impl RothschildService {
            pub fn new(application_events: ApplicationEventsChannel, settings: &Settings) -> Self {
                Self {
                    application_events,
                    service_events: Channel::unbounded(),
                    task_ctl: Channel::oneshot(),
                    is_enabled: AtomicBool::new(settings.node.rothschild_enabled),
                    starting: AtomicBool::new(false),
                    restart_pending: AtomicBool::new(false),
                    logs: Mutex::new(Vec::new()),
                    network: Mutex::new(settings.node.network),
                    settings: Mutex::new(settings.node.rothschild.clone()),
                    child: Mutex::new(None),
                }
            }

            pub fn enable(
                &self,
                enabled: bool,
                network: Network,
                settings: &RothschildSettings,
            ) {
                self.service_events
                    .try_send(RothschildEvents::SetEnabled {
                        enabled,
                        network,
                        settings: settings.clone(),
                    })
                    .unwrap();
            }

            pub fn update_settings(&self, network: Network, settings: &RothschildSettings) {
                self.service_events
                    .try_send(RothschildEvents::UpdateSettings {
                        network,
                        settings: settings.clone(),
                    })
                    .unwrap();
            }

            pub fn logs(&self) -> MutexGuard<'_, Vec<Log>> {
                self.logs.lock().unwrap()
            }

            async fn update_logs(&self, line: String) {
                {
                    let mut logs = self.logs.lock().unwrap();
                    if logs.len() > LOG_BUFFER_LINES {
                        logs.drain(0..LOG_BUFFER_MARGIN);
                    }
                    logs.push(line.as_str().into());
                }

                if update_logs_flag().load(Ordering::SeqCst) && crate::runtime::try_runtime().is_some() {
                    self.application_events
                        .sender
                        .send(Events::UpdateLogs)
                        .await
                        .unwrap();
                }
            }

            fn is_running(&self) -> bool {
                self.child.lock().unwrap().is_some()
            }

            fn rothschild_binary_name() -> &'static str {
                if cfg!(windows) { "rothschild.exe" } else { "rothschild" }
            }

            fn find_rothschild_binary() -> Option<PathBuf> {
                let bin_name = Self::rothschild_binary_name();

                if let Ok(exe) = std::env::current_exe() {
                    if let Some(dir) = exe.parent() {
                        let candidate = dir.join(bin_name);
                        if candidate.exists() {
                            return Some(candidate);
                        }
                    }
                }

                if let Ok(cwd) = std::env::current_dir() {
                    for profile in ["debug", "release"] {
                        let candidate = cwd.join("target").join(profile).join(bin_name);
                        if candidate.exists() {
                            return Some(candidate);
                        }
                        let candidate = cwd.join("rusty-kaspa").join("target").join(profile).join(bin_name);
                        if candidate.exists() {
                            return Some(candidate);
                        }
                    }
                }

                None
            }

            fn schedule_restart(self: &Arc<Self>, reason: &str) {
                if !self.is_enabled.load(Ordering::SeqCst) {
                    return;
                }

                if !is_supported_network(*self.network.lock().unwrap()) {
                    return;
                }

                if self.restart_pending.swap(true, Ordering::SeqCst) {
                    return;
                }

                let this = Arc::clone(self);
                let reason = reason.to_string();
                tokio::spawn(async move {
                    this.update_logs(format!(
                        "Rothschild: {reason}; restarting in {}s",
                        RESTART_DELAY.as_secs()
                    ))
                    .await;
                    task::sleep(RESTART_DELAY).await;
                    if !this.is_enabled.load(Ordering::SeqCst) {
                        this.restart_pending.store(false, Ordering::SeqCst);
                        return;
                    }
                    if !is_supported_network(*this.network.lock().unwrap()) {
                        this.restart_pending.store(false, Ordering::SeqCst);
                        return;
                    }
                    this.restart_pending.store(false, Ordering::SeqCst);
                    let _ = this.start_rothschild().await;
                });
            }

            async fn start_rothschild(self: &Arc<Self>) -> Result<()> {
                if self.is_running() {
                    return Ok(());
                }

                if self.starting.swap(true, Ordering::SeqCst) {
                    return Ok(());
                }

                struct StartGuard<'a>(&'a AtomicBool);
                impl Drop for StartGuard<'_> {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::SeqCst);
                    }
                }

                let _guard = StartGuard(&self.starting);

                let network = *self.network.lock().unwrap();
                if !is_supported_network(network) {
                    self.update_logs(
                        i18n("Rothschild: available only on Testnet 10 and Testnet 12.")
                            .to_string(),
                    )
                    .await;
                    return Ok(());
                }

                let settings = self.settings.lock().unwrap().clone();
                let private_key = settings.private_key.trim();
                let address = settings.address.trim();
                let restart_on_exit = !private_key.is_empty() && !address.is_empty();

                if private_key.is_empty() {
                    self.update_logs(i18n("Rothschild: private key is not set (configure it in Settings).").to_string()).await;
                    return Ok(());
                }

                if address.is_empty() {
                    self.update_logs(i18n("Rothschild: address is not set (configure it in Settings).").to_string()).await;
                    return Ok(());
                }

                let rothschild_bin = match Self::find_rothschild_binary() {
                    Some(bin) => bin,
                    None => {
                        self.update_logs(i18n("Rothschild: binary not found (build rothschild first)").to_string()).await;
                        self.schedule_restart("binary not found");
                        return Ok(());
                    }
                };

                let mut cmd = Command::new(rothschild_bin);
                cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

                let tps = settings.tps.max(1);
                cmd.arg("--tps").arg(tps.to_string());

                if !settings.rpc_server.trim().is_empty() {
                    cmd.arg("--rpcserver").arg(settings.rpc_server.trim());
                }

                if settings.threads > 0 {
                    cmd.arg("--threads").arg(settings.threads.to_string());
                }

                cmd.arg("--private-key").arg(private_key);
                cmd.arg("--to-addr").arg(address);

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
                        self.update_logs(format!("Rothschild: failed to start ({})", err)).await;
                        self.schedule_restart("failed to start");
                        return Err(err);
                    }
                };

                if let Some(stdout) = child.stdout.take() {
                    let this = Arc::clone(self);
                    tokio::spawn(async move {
                        let mut reader = BufReader::new(stdout).lines();
                        while let Ok(Some(line)) = reader.next_line().await {
                            this.update_logs(line).await;
                        }
                    });
                }

                if let Some(stderr) = child.stderr.take() {
                    let this = Arc::clone(self);
                    tokio::spawn(async move {
                        let mut reader = BufReader::new(stderr).lines();
                        while let Ok(Some(line)) = reader.next_line().await {
                            this.update_logs(line).await;
                        }
                    });
                }

                *self.child.lock().unwrap() = Some(child);
                if restart_on_exit {
                    self.update_logs(i18n("Rothschild: started").to_string()).await;
                } else {
                    self.update_logs(i18n("Rothschild: started (wallet generation)").to_string()).await;
                }

                let monitor = Arc::clone(self);
                tokio::spawn(async move {
                    loop {
                        task::sleep(Duration::from_secs(1)).await;
                        let status = {
                            let mut guard = monitor.child.lock().unwrap();
                            match guard.as_mut() {
                                Some(child) => child.try_wait(),
                                None => return,
                            }
                        };

                        match status {
                            Ok(Some(status)) => {
                                monitor.child.lock().unwrap().take();
                                let _ = monitor.update_logs(format!("Rothschild: exited ({})", status)).await;
                                if restart_on_exit || !status.success() {
                                    monitor.schedule_restart("rothschild exited");
                                }
                                return;
                            }
                            Ok(None) => {}
                            Err(err) => {
                                monitor.child.lock().unwrap().take();
                                let _ = monitor.update_logs(format!("Rothschild: monitor error ({})", err)).await;
                                monitor.schedule_restart("rothschild monitor error");
                                return;
                            }
                        }
                    }
                });

                Ok(())
            }

            async fn stop_rothschild(self: &Arc<Self>) -> Result<()> {
                let child = self.child.lock().unwrap().take();
                if let Some(mut child) = child {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    self.update_logs(i18n("Rothschild: stopped").to_string()).await;
                }
                Ok(())
            }
        }

        #[async_trait]
        impl Service for RothschildService {
            fn name(&self) -> &'static str {
                "rothschild-service"
            }

            async fn spawn(self: Arc<Self>) -> Result<()> {
                let this = self.clone();
                tokio::spawn(async move {
                    if this.is_enabled.load(Ordering::SeqCst) {
                        let _ = this.start_rothschild().await;
                    }

                    loop {
                        select! {
                            msg = this.service_events.receiver.recv().fuse() => {
                                match msg {
                                    Ok(RothschildEvents::SetEnabled { enabled, network, settings }) => {
                                        this.is_enabled.store(enabled, Ordering::SeqCst);
                                        *this.network.lock().unwrap() = network;
                                        *this.settings.lock().unwrap() = settings;
                                        if enabled {
                                            let _ = this.stop_rothschild().await;
                                            let _ = this.start_rothschild().await;
                                        } else {
                                            let _ = this.stop_rothschild().await;
                                        }
                                    }
                                    Ok(RothschildEvents::UpdateSettings { network, settings }) => {
                                        *this.network.lock().unwrap() = network;
                                        *this.settings.lock().unwrap() = settings;
                                        if this.is_enabled.load(Ordering::SeqCst) {
                                            let _ = this.stop_rothschild().await;
                                            let _ = this.start_rothschild().await;
                                        }
                                    }
                                    Ok(RothschildEvents::Exit) | Err(_) => {
                                        let _ = this.stop_rothschild().await;
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
                self.service_events.sender.try_send(RothschildEvents::Exit).unwrap();
            }

            async fn join(self: Arc<Self>) -> Result<()> {
                self.task_ctl.recv().await.unwrap();
                Ok(())
            }
        }
    } else {
        pub struct RothschildService;

        impl RothschildService {
            pub fn new(_application_events: ApplicationEventsChannel, _settings: &Settings) -> Self {
                Self
            }

            pub fn enable(
                &self,
                _enabled: bool,
                _network: Network,
                _settings: &RothschildSettings,
            ) {}

            pub fn update_settings(&self, _network: Network, _settings: &RothschildSettings) {}
        }

        pub fn update_logs_flag() -> &'static Arc<AtomicBool> {
            static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();
            FLAG.get_or_init(|| Arc::new(AtomicBool::new(false)))
        }

        #[async_trait]
        impl Service for RothschildService {
            fn name(&self) -> &'static str {
                "rothschild-service"
            }

            async fn spawn(self: Arc<Self>) -> Result<()> {
                Ok(())
            }

            fn terminate(self: Arc<Self>) {}

            async fn join(self: Arc<Self>) -> Result<()> {
                Ok(())
            }
        }
    }
}
