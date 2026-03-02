use crate::imports::*;
use crate::runtime::Service;

cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        use crate::runtime::services::kaspa::logs::Log;
        use std::collections::HashMap;
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
        use tokio::net::TcpStream;
        use tokio::process::{Child, Command};

        const LOG_BUFFER_LINES: usize = 4096;
        const LOG_BUFFER_MARGIN: usize = 128;
        const BLOCK_BUFFER_LINES: usize = 256;
        const BLOCK_BUFFER_MARGIN: usize = 32;
        const RESTART_DELAY: Duration = Duration::from_secs(3);
        const BRIDGE_PROM_PORT: u16 = 2114;
        const STATS_REFRESH_INTERVAL: Duration = Duration::from_secs(5);
        const STATS_WORKER_WIDTH: usize = 16;
        const STATS_WALLET_WIDTH: usize = 18;
        const STATS_INST_WIDTH: usize = 5;
        const STATS_HASH_WIDTH: usize = 11;
        const STATS_DIFF_WIDTH: usize = 6;
        const STATS_SPM_WIDTH: usize = 11;
        const STATS_TRND_WIDTH: usize = 4;
        const STATS_ACC_WIDTH: usize = 12;
        const STATS_BLK_WIDTH: usize = 6;
        const STATS_TIME_WIDTH: usize = 11;

        fn default_grpc_port_for_network(network: Network) -> u16 {
            crate::settings::node_grpc_port_for_network(network)
        }

        fn local_grpc_ports() -> [u16; 3] {
            [
                crate::settings::node_grpc_port_for_network(Network::Mainnet),
                crate::settings::node_grpc_port_for_network(Network::Mainnet),
                crate::settings::node_grpc_port_for_network(Network::Mainnet),
            ]
        }

        fn is_local_host(host: &str) -> bool {
            matches!(
                host.trim().to_ascii_lowercase().as_str(),
                "127.0.0.1" | "localhost" | "0.0.0.0" | "::1" | "::" | "[::1]" | "[::]"
            )
        }

        fn is_bridge_table_line(line: &str) -> bool {
            let trimmed = line.trim_start();
            trimmed.starts_with('+') || trimmed.starts_with('|')
        }

        #[derive(Debug, Clone, Deserialize)]
        struct BridgeStatsWorker {
            worker: String,
            wallet: String,
        }

        #[derive(Debug, Clone, Deserialize)]
        struct BridgeStatsResponse {
            workers: Vec<BridgeStatsWorker>,
        }

        pub fn update_logs_flag() -> &'static Arc<AtomicBool> {
            static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();
            FLAG.get_or_init(|| Arc::new(AtomicBool::new(false)))
        }

        #[derive(Debug, Clone)]
        pub struct BridgeBlock {
            pub timestamp: Option<String>,
            pub hash: String,
            pub status: String,
            pub worker: Option<String>,
            pub wallet: Option<String>,
            pub line: String,
        }

        #[derive(Debug, Clone)]
        struct BlockDetails {
            hash: Option<String>,
            worker: Option<String>,
            wallet: Option<String>,
        }

        #[derive(Debug, Clone)]
        enum BridgeEvents {
            SetEnabled { enabled: bool, settings: NodeSettings },
            UpdateSettings(NodeSettings),
            Exit,
        }

        pub struct StratumBridgeService {
            application_events: ApplicationEventsChannel,
            service_events: Channel<BridgeEvents>,
            task_ctl: Channel<()>,
            is_enabled: AtomicBool,
            starting: AtomicBool,
            restart_pending: AtomicBool,
            logs: Mutex<Vec<Log>>,
            blocks: Mutex<Vec<BridgeBlock>>,
            worker_wallets: Mutex<HashMap<String, String>>,
            last_block_hash: Mutex<Option<String>>,
            node_settings: Mutex<NodeSettings>,
            child: Mutex<Option<Child>>,
        }

        impl StratumBridgeService {
            pub fn new(application_events: ApplicationEventsChannel, settings: &Settings) -> Self {
                let enabled = settings.node.stratum_bridge_enabled && settings.node.node_kind.is_local();
                Self {
                    application_events,
                    service_events: Channel::unbounded(),
                    task_ctl: Channel::oneshot(),
                    is_enabled: AtomicBool::new(enabled),
                    starting: AtomicBool::new(false),
                    restart_pending: AtomicBool::new(false),
                    logs: Mutex::new(Vec::new()),
                    blocks: Mutex::new(Vec::new()),
                    worker_wallets: Mutex::new(HashMap::new()),
                    last_block_hash: Mutex::new(None),
                    node_settings: Mutex::new(settings.node.clone()),
                    child: Mutex::new(None),
                }
            }

            pub fn enable(&self, enabled: bool, node_settings: &NodeSettings) {
                self.service_events
                    .try_send(BridgeEvents::SetEnabled {
                        enabled,
                        settings: node_settings.clone(),
                    })
                    .unwrap();
            }

            pub fn update_settings(&self, node_settings: &NodeSettings) {
                self.service_events
                    .try_send(BridgeEvents::UpdateSettings(node_settings.clone()))
                    .unwrap();
            }

            pub fn logs(&self) -> MutexGuard<'_, Vec<Log>> {
                self.logs.lock().unwrap()
            }

            pub fn blocks(&self) -> MutexGuard<'_, Vec<BridgeBlock>> {
                self.blocks.lock().unwrap()
            }

            async fn update_logs(&self, line: String) {
                let display_line = self.rewrite_stats_table_line(&line);
                {
                    let mut logs = self.logs.lock().unwrap();
                    if logs.len() > LOG_BUFFER_LINES {
                        logs.drain(0..LOG_BUFFER_MARGIN);
                    }
                    if is_bridge_table_line(&display_line) {
                        logs.push(Log::Processed(display_line.clone()));
                    } else {
                        logs.push(display_line.as_str().into());
                    }
                }

                if let Some(block_event) = Self::parse_block_event(&line) {
                    self.set_last_block_hash(block_event.hash.as_str());
                    self.record_block(block_event);
                }

                if let Some(details) = Self::parse_block_details(&line) {
                    self.apply_block_details(details);
                }

                if update_logs_flag().load(Ordering::SeqCst) && crate::runtime::try_runtime().is_some() {
                    self.application_events
                        .sender
                        .send(Events::UpdateLogs)
                        .await
                        .unwrap();
                }
            }

            fn record_block(&self, event: BridgeBlock) {
                let mut blocks = self.blocks.lock().unwrap();
                if blocks.len() > BLOCK_BUFFER_LINES {
                    blocks.drain(0..BLOCK_BUFFER_MARGIN);
                }

                if let Some(existing) = blocks.iter_mut().rev().find(|block| block.hash == event.hash) {
                    existing.status = event.status;
                    if event.timestamp.is_some() {
                        existing.timestamp = event.timestamp;
                    }
                    if event.worker.is_some() {
                        existing.worker = event.worker;
                    }
                    if event.wallet.is_some() {
                        existing.wallet = event.wallet;
                    }
                    existing.line = event.line;
                } else {
                    blocks.push(event);
                }
            }

            fn set_last_block_hash(&self, hash: &str) {
                *self.last_block_hash.lock().unwrap() = Some(hash.to_string());
            }

            fn apply_block_details(&self, details: BlockDetails) {
                let mut hash = details.hash;
                if hash.is_none() {
                    hash = self.last_block_hash.lock().unwrap().clone();
                }
                let Some(hash) = hash else { return; };

                let mut blocks = self.blocks.lock().unwrap();
                if let Some(existing) = blocks.iter_mut().rev().find(|block| block.hash == hash) {
                    if details.worker.is_some() {
                        existing.worker = details.worker;
                    }
                    if details.wallet.is_some() {
                        existing.wallet = details.wallet;
                    }
                } else {
                    blocks.push(BridgeBlock {
                        timestamp: None,
                        hash,
                        status: "Found".to_string(),
                        worker: details.worker,
                        wallet: details.wallet,
                        line: String::new(),
                    });
                }
            }

            fn parse_block_event(line: &str) -> Option<BridgeBlock> {
                let status = if line.contains("BLOCK FOUND!") {
                    "Found".to_string()
                } else if line.contains("BLOCK ACCEPTED BY KASPA NODE") {
                    "Accepted".to_string()
                } else if line.contains("BLOCK REJECTED BY KASPA NODE") {
                    if line.contains("STALE") {
                        "Rejected (Stale)".to_string()
                    } else if line.contains("INVALID") {
                        "Rejected (Invalid)".to_string()
                    } else {
                        "Rejected".to_string()
                    }
                } else {
                    return None;
                };

                let hash = Self::extract_block_hash(line)?;
                let timestamp = Self::extract_timestamp(line);
                let details = Self::parse_block_details(line);

                Some(BridgeBlock {
                    timestamp,
                    hash,
                    status,
                    worker: details.as_ref().and_then(|d| d.worker.clone()),
                    wallet: details.and_then(|d| d.wallet),
                    line: line.to_string(),
                })
            }

            fn parse_block_details(line: &str) -> Option<BlockDetails> {
                let hash = Self::extract_block_hash(line);
                let worker = Self::extract_field_any(
                    line,
                    &["Worker:", "worker:", "Worker=", "worker="],
                    &[
                        " Wallet",
                        " wallet",
                        ",",
                        " Nonce",
                        " nonce",
                        " Pow",
                        " pow",
                        " Hash",
                        " hash",
                    ],
                );
                let wallet = Self::extract_field_any(
                    line,
                    &["Wallet:", "wallet:", "Wallet=", "wallet="],
                    &[",", " Nonce", " nonce", " Pow", " pow", " Hash", " hash"],
                );

                if hash.is_none() && worker.is_none() && wallet.is_none() {
                    None
                } else {
                    Some(BlockDetails { hash, worker, wallet })
                }
            }

            fn extract_field_any(line: &str, labels: &[&str], terminators: &[&str]) -> Option<String> {
                labels
                    .iter()
                    .find_map(|label| Self::extract_field(line, label, terminators))
            }

            fn extract_field(line: &str, label: &str, terminators: &[&str]) -> Option<String> {
                let idx = line.find(label)?;
                let mut rest = line[idx + label.len()..].trim_start();
                let mut end = rest.len();
                for term in terminators {
                    if let Some(pos) = rest.find(term) {
                        end = end.min(pos);
                    }
                }
                rest = rest[..end].trim();
                let cleaned = rest.trim_matches(|c: char| c == ',' || c == ')' || c == ']' || c == '"');
                if cleaned.is_empty() {
                    None
                } else {
                    Some(cleaned.to_string())
                }
            }

            fn extract_block_hash(line: &str) -> Option<String> {
                let idx = line.find("Hash:")?;
                let rest = &line[idx + "Hash:".len()..];
                let hash = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches(|c: char| c == '"' || c == ',' || c == ')' || c == ']');
                if hash.is_empty() {
                    None
                } else {
                    Some(hash.to_string())
                }
            }

            fn extract_timestamp(line: &str) -> Option<String> {
                let token = line.split_whitespace().next()?;
                if token.contains(':')
                    && token
                        .chars()
                        .all(|c| c.is_ascii_digit() || matches!(c, ':' | '.' | '-' | 'T' | 'Z'))
                {
                    Some(token.to_string())
                } else {
                    None
                }
            }

            fn truncate_cell(value: &str, width: usize) -> String {
                let value = value.trim();
                let chars = value.chars().count();
                if chars <= width {
                    return value.to_string();
                }
                if width <= 3 {
                    return value.chars().take(width).collect();
                }
                let head: String = value.chars().take(width - 3).collect();
                format!("{head}...")
            }

            fn short_wallet_label(value: &str) -> String {
                let wallet = value.trim();
                if wallet.is_empty() {
                    return String::new();
                }

                let (prefix, payload) = if let Some((prefix, payload)) = wallet.split_once(':') {
                    (prefix, payload)
                } else {
                    ("wallet", wallet)
                };
                let normalized_prefix = prefix.trim().to_ascii_lowercase();
                let compact = payload.trim();
                let first: String = compact.chars().take(4).collect();
                let last: String = compact
                    .chars()
                    .rev()
                    .take(4)
                    .collect::<Vec<char>>()
                    .into_iter()
                    .rev()
                    .collect();
                if first.is_empty() || last.is_empty() {
                    format!("{normalized_prefix}:{compact}")
                } else {
                    format!("{normalized_prefix}:{first}...{last}")
                }
            }

            fn stats_border_line() -> String {
                format!(
                    "+-{}-+-{}-+-{}-+-{}-+-{}-+-{}-+-{}-+-{}-+-{}-+-{}-+",
                    "-".repeat(STATS_WORKER_WIDTH),
                    "-".repeat(STATS_WALLET_WIDTH),
                    "-".repeat(STATS_INST_WIDTH),
                    "-".repeat(STATS_HASH_WIDTH),
                    "-".repeat(STATS_DIFF_WIDTH),
                    "-".repeat(STATS_SPM_WIDTH),
                    "-".repeat(STATS_TRND_WIDTH),
                    "-".repeat(STATS_ACC_WIDTH),
                    "-".repeat(STATS_BLK_WIDTH),
                    "-".repeat(STATS_TIME_WIDTH)
                )
            }

            fn stats_header_line() -> String {
                format!(
                    "| {:<STATS_WORKER_WIDTH$} | {:<STATS_WALLET_WIDTH$} | {:<STATS_INST_WIDTH$} | {:>STATS_HASH_WIDTH$} | {:>STATS_DIFF_WIDTH$} | {:>STATS_SPM_WIDTH$} | {:<STATS_TRND_WIDTH$} | {:>STATS_ACC_WIDTH$} | {:>STATS_BLK_WIDTH$} | {:>STATS_TIME_WIDTH$} |",
                    "Worker", "Wallet", "Inst", "Hash", "Diff", "SPM|TGT", "Trnd", "Acc|Stl|Inv", "Blocks", "D|HR|M|S",
                )
            }

            fn resolve_wallet_for_worker(&self, worker: &str) -> Option<String> {
                let worker = worker.trim();
                if worker.is_empty() {
                    return None;
                }
                let map = self.worker_wallets.lock().unwrap();
                if let Some(wallet) = map.get(worker) {
                    return Some(wallet.clone());
                }
                map.iter().find_map(|(known_worker, wallet)| {
                    if known_worker.starts_with(worker) || worker.starts_with(known_worker) {
                        Some(wallet.clone())
                    } else {
                        None
                    }
                })
            }

            fn rewrite_stats_table_line(&self, line: &str) -> String {
                let trimmed = line.trim_start();
                if !(trimmed.starts_with('|') || trimmed.starts_with('+')) {
                    return line.to_string();
                }

                if trimmed.starts_with('+') && trimmed.contains("-+-") {
                    return Self::stats_border_line();
                }

                if trimmed.contains("| Worker") && trimmed.contains("| Inst") {
                    return Self::stats_header_line();
                }

                if !trimmed.starts_with('|') {
                    return line.to_string();
                }

                let columns: Vec<String> = trimmed
                    .trim_matches('|')
                    .split('|')
                    .map(|part| part.trim().to_string())
                    .collect();
                if columns.len() != 9 {
                    return line.to_string();
                }

                let worker = columns[0].clone();
                let inst = columns[1].clone();
                let wallet_short = self
                    .resolve_wallet_for_worker(worker.as_str())
                    .map(|wallet| Self::short_wallet_label(wallet.as_str()))
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "-".to_string());

                format!(
                    "| {:<STATS_WORKER_WIDTH$} | {:<STATS_WALLET_WIDTH$} | {:<STATS_INST_WIDTH$} | {:>STATS_HASH_WIDTH$} | {:>STATS_DIFF_WIDTH$} | {:>STATS_SPM_WIDTH$} | {:<STATS_TRND_WIDTH$} | {:>STATS_ACC_WIDTH$} | {:>STATS_BLK_WIDTH$} | {:>STATS_TIME_WIDTH$} |",
                    Self::truncate_cell(worker.as_str(), STATS_WORKER_WIDTH),
                    Self::truncate_cell(wallet_short.as_str(), STATS_WALLET_WIDTH),
                    Self::truncate_cell(inst.as_str(), STATS_INST_WIDTH),
                    Self::truncate_cell(columns[2].as_str(), STATS_HASH_WIDTH),
                    Self::truncate_cell(columns[3].as_str(), STATS_DIFF_WIDTH),
                    Self::truncate_cell(columns[4].as_str(), STATS_SPM_WIDTH),
                    Self::truncate_cell(columns[5].as_str(), STATS_TRND_WIDTH),
                    Self::truncate_cell(columns[6].as_str(), STATS_ACC_WIDTH),
                    Self::truncate_cell(columns[7].as_str(), STATS_BLK_WIDTH),
                    Self::truncate_cell(columns[8].as_str(), STATS_TIME_WIDTH),
                )
            }

            fn stats_http_addr() -> String {
                format!("127.0.0.1:{BRIDGE_PROM_PORT}")
            }

            async fn refresh_worker_wallets(&self) {
                let mut stream = match TcpStream::connect(Self::stats_http_addr()).await {
                    Ok(stream) => stream,
                    Err(_) => return,
                };

                let request = b"GET /api/stats HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
                if stream.write_all(request).await.is_err() {
                    return;
                }

                let mut bytes = Vec::new();
                if stream.read_to_end(&mut bytes).await.is_err() {
                    return;
                }

                let response = String::from_utf8_lossy(&bytes);
                let Some((_, body)) = response.split_once("\r\n\r\n") else {
                    return;
                };

                let Ok(stats) = serde_json::from_str::<BridgeStatsResponse>(body) else {
                    return;
                };

                let mut next = HashMap::<String, String>::new();
                for worker in stats.workers {
                    let worker_name = worker.worker.trim();
                    let wallet = worker.wallet.trim();
                    if worker_name.is_empty() || wallet.is_empty() {
                        continue;
                    }
                    next.insert(worker_name.to_string(), wallet.to_string());
                }

                if !next.is_empty() {
                    *self.worker_wallets.lock().unwrap() = next;
                }
            }

            fn grpc_address_from_settings(settings: &NodeSettings) -> Option<String> {
                if !settings.enable_grpc {
                    return None;
                }
                let default_port = default_grpc_port_for_network(settings.network);

                let addr = match settings.grpc_network_interface.kind {
                    NetworkInterfaceKind::Local => "127.0.0.1".to_string(),
                    NetworkInterfaceKind::Any => "127.0.0.1".to_string(),
                    NetworkInterfaceKind::Custom => settings.grpc_network_interface.custom.to_string(),
                };

                if let Some((host, port)) = addr.rsplit_once(':')
                    && let Ok(port) = port.parse::<u16>()
                    && is_local_host(host)
                    && local_grpc_ports().contains(&port)
                    && port != default_port
                {
                    Some(format!("{host}:{default_port}"))
                } else if addr.contains(':') {
                    Some(addr)
                } else {
                    Some(format!("{addr}:{default_port}"))
                }
            }

            fn bridge_binary_name() -> &'static str {
                if cfg!(windows) { "stratum-bridge.exe" } else { "stratum-bridge" }
            }

            fn running_from_macos_bundle() -> bool {
                #[cfg(target_os = "macos")]
                {
                    if let Ok(exe) = std::env::current_exe() {
                        return exe
                            .to_string_lossy()
                            .contains(".app/Contents/MacOS/");
                    }
                }
                false
            }

            fn find_bridge_binary() -> Option<PathBuf> {
                let bin_name = Self::bridge_binary_name();

                if let Ok(exe) = std::env::current_exe() {
                    if let Some(dir) = exe.parent() {
                        let candidate = dir.join(bin_name);
                        if candidate.exists() {
                            return Some(candidate);
                        }
                    }
                }

                if !Self::running_from_macos_bundle() && let Ok(cwd) = std::env::current_dir() {
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

            fn escape_yaml_str(value: &str) -> String {
                value
                    .replace(['\r', '\n'], " ")
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
            }

            fn sanitize_grpc_address(address: &str) -> String {
                let trimmed = address.trim();
                let without_scheme = trimmed
                    .split_once("://")
                    .map(|(_, rest)| rest)
                    .unwrap_or(trimmed);
                without_scheme.trim_end_matches('/').to_string()
            }

            fn normalize_port(value: &str) -> String {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return String::new();
                }
                if trimmed.starts_with(':') {
                    trimmed.to_string()
                } else if trimmed.chars().all(|c| c.is_ascii_digit()) {
                    format!(":{}", trimmed)
                } else {
                    trimmed.to_string()
                }
            }

            fn write_bridge_config(kaspad_address: &str, bridge: &StratumBridgeSettings) -> Result<PathBuf> {
                let mut path = std::env::temp_dir();
                path.push("kaspa-ng-bridge-config.yaml");

                let stratum_port = {
                    let port = Self::normalize_port(&bridge.stratum_port);
                    if port.is_empty() { ":5555".to_string() } else { port }
                };
                let health_check_port = Self::normalize_port(&bridge.health_check_port);
                let block_wait_time_ms = bridge.block_wait_time_ms.max(1);
                let min_share_diff = bridge.min_share_diff.max(1);
                let shares_per_min = bridge.shares_per_min.max(1);
                let extranonce_size = bridge.extranonce_size.min(3);
                let coinbase_tag_suffix = Self::escape_yaml_str(bridge.coinbase_tag_suffix.trim());
                let kaspad_address = Self::escape_yaml_str(kaspad_address);

                let contents = format!(
                    r#"# Autogenerated by Kaspa-NG
kaspad_address: "{kaspad_address}"
block_wait_time: {block_wait_time_ms}
print_stats: {print_stats}
log_to_file: {log_to_file}
health_check_port: "{health_check_port}"
var_diff: {var_diff}
shares_per_min: {shares_per_min}
var_diff_stats: {var_diff_stats}
pow2_clamp: {pow2_clamp}
extranonce_size: {extranonce_size}
coinbase_tag_suffix: "{coinbase_tag_suffix}"

instances:
  - stratum_port: "{stratum_port}"
    prom_port: ":{prom_port}"
    min_share_diff: {min_share_diff}
"#
                    ,
                    block_wait_time_ms = block_wait_time_ms,
                    print_stats = bridge.print_stats,
                    log_to_file = bridge.log_to_file,
                    health_check_port = health_check_port,
                    var_diff = bridge.var_diff,
                    shares_per_min = shares_per_min,
                    var_diff_stats = bridge.var_diff_stats,
                    pow2_clamp = bridge.pow2_clamp,
                    extranonce_size = extranonce_size,
                    coinbase_tag_suffix = coinbase_tag_suffix,
                    prom_port = BRIDGE_PROM_PORT,
                    stratum_port = stratum_port,
                    min_share_diff = min_share_diff,
                );

                std::fs::write(&path, contents)?;
                Ok(path)
            }

            fn is_running(&self) -> bool {
                self.child.lock().unwrap().is_some()
            }

            async fn wait_for_grpc(self: &Arc<Self>, address: &str) -> bool {
                let address = Self::sanitize_grpc_address(address);
                let mut attempts: u32 = 0;
                loop {
                    if !self.is_enabled.load(Ordering::SeqCst) {
                        return false;
                    }

                    match TcpStream::connect(address.as_str()).await {
                        Ok(_) => return true,
                        Err(_) => {
                            if attempts == 0 {
                                self.update_logs(format!(
                                    "RK Bridge: waiting for Kaspa node gRPC at {}",
                                    address
                                ))
                                .await;
                            } else if attempts % 10 == 0 {
                                self.update_logs(format!(
                                    "RK Bridge: still waiting for Kaspa node gRPC at {}",
                                    address
                                ))
                                .await;
                            }
                            attempts = attempts.saturating_add(1);
                            task::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }

            fn schedule_restart(self: &Arc<Self>, reason: &str) {
                if !self.is_enabled.load(Ordering::SeqCst) {
                    return;
                }

                if self.restart_pending.swap(true, Ordering::SeqCst) {
                    return;
                }

                let this = Arc::clone(self);
                let reason = reason.to_string();
                tokio::spawn(async move {
                    this.update_logs(format!(
                        "RK Bridge: {reason}; restarting in {}s",
                        RESTART_DELAY.as_secs()
                    ))
                    .await;
                    task::sleep(RESTART_DELAY).await;
                    if !this.is_enabled.load(Ordering::SeqCst) {
                        this.restart_pending.store(false, Ordering::SeqCst);
                        return;
                    }
                    this.restart_pending.store(false, Ordering::SeqCst);
                    let _ = this.start_bridge().await;
                });
            }

            async fn start_bridge(self: &Arc<Self>) -> Result<()> {
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

                let settings = self.node_settings.lock().unwrap().clone();
                if !settings.node_kind.is_local() {
                    self.update_logs(i18n("RK Bridge: disabled (node is not local)").to_string()).await;
                    return Ok(());
                }

                let kaspad_address = match Self::grpc_address_from_settings(&settings) {
                    Some(addr) => addr,
                    None => {
                        self.update_logs(i18n("RK Bridge: gRPC is disabled; enable gRPC to start the bridge.").to_string()).await;
                        return Ok(());
                    }
                };

                let bridge_bin = match Self::find_bridge_binary() {
                    Some(path) => path,
                    None => {
                        self.update_logs(i18n("RK Bridge: stratum-bridge binary not found (build kaspa-stratum-bridge first)").to_string()).await;
                        self.schedule_restart("stratum-bridge binary not found");
                        return Ok(());
                    }
                };

                if !self.wait_for_grpc(&kaspad_address).await {
                    return Ok(());
                }

                let config_path = Self::write_bridge_config(&kaspad_address, &settings.stratum_bridge)?;

                let mut cmd = Command::new(bridge_bin);
                cmd.arg("--config")
                    .arg(config_path)
                    .arg("--node-mode")
                    .arg("external")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                #[cfg(windows)]
                {
                    const CREATE_NO_WINDOW: u32 = 0x08000000;
                    cmd.creation_flags(CREATE_NO_WINDOW);
                }

                let mut child = match cmd.spawn() {
                    Ok(child) => child,
                    Err(err) => {
                        let err = Error::NodeStartupError(err);
                        self.update_logs(format!("RK Bridge: failed to start ({})", err))
                            .await;
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
                self.update_logs(i18n("RK Bridge: started in external mode").to_string()).await;

                let stats_sync = Arc::clone(self);
                tokio::spawn(async move {
                    let mut tick = tokio::time::interval(STATS_REFRESH_INTERVAL);
                    loop {
                        tick.tick().await;
                        if !stats_sync.is_enabled.load(Ordering::SeqCst) || !stats_sync.is_running() {
                            return;
                        }
                        stats_sync.refresh_worker_wallets().await;
                    }
                });

                // Monitor child exit and clear handle
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
                                let _ = monitor
                                    .update_logs(format!("RK Bridge: exited ({})", status))
                                    .await;
                                if status.success() {
                                    let _ = monitor
                                        .update_logs(
                                            "RK Bridge: exited cleanly; automatic restart skipped.".to_string(),
                                        )
                                        .await;
                                } else {
                                    monitor.schedule_restart("bridge exited with error");
                                }
                                return;
                            }
                            Ok(None) => {}
                            Err(err) => {
                                monitor.child.lock().unwrap().take();
                                let _ = monitor
                                    .update_logs(format!("RK Bridge: monitor error ({})", err))
                                    .await;
                                monitor.schedule_restart("bridge monitor error");
                                return;
                            }
                        }
                    }
                });
                Ok(())
            }

            async fn stop_bridge(self: &Arc<Self>) -> Result<()> {
                let child = self.child.lock().unwrap().take();
                if let Some(mut child) = child {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    self.update_logs(i18n("RK Bridge: stopped").to_string()).await;
                }
                Ok(())
            }
        }

        #[async_trait]
        impl Service for StratumBridgeService {
            fn name(&self) -> &'static str {
                "stratum-bridge-service"
            }

            async fn spawn(self: Arc<Self>) -> Result<()> {
                let this = self.clone();
                tokio::spawn(async move {
                    if this.is_enabled.load(Ordering::SeqCst) {
                        let _ = this.start_bridge().await;
                    }

                    loop {
                        select! {
                            msg = this.service_events.receiver.recv().fuse() => {
                                match msg {
                                    Ok(BridgeEvents::SetEnabled { enabled, settings }) => {
                                        this.is_enabled.store(enabled, Ordering::SeqCst);
                                        *this.node_settings.lock().unwrap() = settings;
                                        if enabled {
                                            let _ = this.start_bridge().await;
                                        } else {
                                            let _ = this.stop_bridge().await;
                                        }
                                    }
                                    Ok(BridgeEvents::UpdateSettings(settings)) => {
                                        *this.node_settings.lock().unwrap() = settings;
                                        if this.is_enabled.load(Ordering::SeqCst) {
                                            let _ = this.stop_bridge().await;
                                            let _ = this.start_bridge().await;
                                        }
                                    }
                                    Ok(BridgeEvents::Exit) | Err(_) => {
                                        let _ = this.stop_bridge().await;
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
                self.service_events.sender.try_send(BridgeEvents::Exit).unwrap();
            }

            async fn join(self: Arc<Self>) -> Result<()> {
                self.task_ctl.recv().await.unwrap();
                Ok(())
            }
        }

    } else {
        pub struct StratumBridgeService;

        impl StratumBridgeService {
            pub fn new(_application_events: ApplicationEventsChannel, _settings: &Settings) -> Self {
                Self
            }

            pub fn enable(&self, _enabled: bool, _node_settings: &NodeSettings) {}

            pub fn update_settings(&self, _node_settings: &NodeSettings) {}
        }

        pub fn update_logs_flag() -> &'static Arc<AtomicBool> {
            static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();
            FLAG.get_or_init(|| Arc::new(AtomicBool::new(false)))
        }

        #[async_trait]
        impl Service for StratumBridgeService {
            fn name(&self) -> &'static str {
                "stratum-bridge-service"
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
