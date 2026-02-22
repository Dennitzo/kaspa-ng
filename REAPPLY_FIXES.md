# Reapply Fixes

This file tracks fixes that may need to be re-applied after upstream changes or rebases.

## 2026-02-22 - Wallet inputs losing focus on Windows
- Symptom: In the Wallet view, text inputs would activate on click and immediately lose focus, making typing impossible (observed on Windows).
- Root cause: `TextEditor` kept a pending focus target in `FocusManager` after auto-focus, so later clicks on other inputs were overridden by a repeated focus request from the earlier field.
- Fix: Request focus once and immediately clear the `FocusManager` to prevent focus stealing on subsequent frames.
- Files touched: `core/src/egui/extensions.rs`.

## 2026-02-22 - Add Testnet 12 as first-class network option
- Symptom: The app only modeled `Mainnet` and `Testnet10`, which blocked incremental TN12 integration work.
- Root cause: `Network` enum, network-to-`NetworkId` conversion, explorer links, and node argument generation were hardcoded to TN10 only.
- Fix: Added `Network::Testnet12` end-to-end (display/parse/serde/network-id suffix 12), added TN12 explorer constant, and updated all affected match arms.
- Files touched: `core/src/network.rs`, `core/src/core.rs`, `core/src/imports.rs`, `core/src/primitives/descriptor.rs`, `core/src/primitives/transaction.rs`, `core/src/modules/overview.rs`, `core/src/modules/block_dag.rs`, `core/src/modules/welcome.rs`, `core/src/runtime/services/kaspa/config.rs`.

## 2026-02-22 - Add explorer endpoint model (official/self-hosted x network)
- Symptom: Explorer endpoint selection was not modeled in settings, which blocked deterministic switching between Mainnet/TN10/TN12 and official/self-hosted backends.
- Root cause: No dedicated typed config existed for explorer API/WS endpoints and source selection.
- Fix: Added `ExplorerSettings` with `ExplorerDataSource` plus network profiles for `Mainnet`, `Testnet10`, and `Testnet12`, including defaults:
  - Official TN10 endpoints: `https://api-tn10.kaspa.org` and `wss://t-2.kaspa.ws`
  - Official TN12 endpoint: `https://api-tn12.kaspa.org`
  - Self-hosted defaults per network: `http://127.0.0.1:8000` + `http://127.0.0.1:8001`
- Note: `core/src/modules/explorer.rs` (WebView/clipboard fixes) was intentionally not modified in this step.
- Files touched: `core/src/settings.rs`, `core/src/imports.rs`.

## 2026-02-22 - Enable CPU Miner and Rothschild tabs for Testnet 10 + Testnet 12
- Symptom: CPU Miner and Rothschild tooling from TN12 integration were missing in the main project, and did not follow selected network in runtime.
- Root cause: Native modules/services were not registered in the main runtime, no settings model existed for both services, and no network-aware service update flow was wired on network changes.
- Fix:
  - Added `CpuMinerSettings` and `RothschildSettings` into persistent node settings.
  - Integrated native modules (`CPU Miner`, `Rothschild`) and runtime services.
  - Added Settings UI sections to configure/enable both services.
  - Added tab visibility logic so both tabs show for `Testnet10` and `Testnet12` (local node context), and hidden outside those networks.
  - Wired network change propagation so both services reconfigure/restart against the currently selected network.
  - Kept Explorer/WebView module untouched.
- Files touched: `core/src/settings.rs`, `core/src/imports.rs`, `core/src/runtime/services/mod.rs`, `core/src/runtime/services/cpu_miner.rs`, `core/src/runtime/services/rothschild.rs`, `core/src/runtime/mod.rs`, `core/src/modules/mod.rs`, `core/src/modules/cpu_miner_logs.rs`, `core/src/modules/rothschild_logs.rs`, `core/src/modules/settings/mod.rs`, `core/src/menu.rs`, `core/src/core.rs`.

## 2026-02-22 - Fix shutdown panic when no Tokio reactor is active
- Symptom: On forced termination (`Ctrl+C`/abort path), app panicked with `there is no reactor running` from `runtime::halt`.
- Root cause: `runtime::halt()` used `tokio::spawn(...)` unconditionally, even when called from a thread without an active Tokio runtime.
- Fix: `runtime::halt()` now:
  - uses `Handle::try_current()` and spawns shutdown only when a runtime exists;
  - otherwise builds a temporary current-thread Tokio runtime and `block_on`s shutdown;
  - logs an error if runtime creation fails.
- Files touched: `core/src/runtime/mod.rs`.

## 2026-02-22 - Remove static `rusty-kaspa:v...` log line from log views
- Symptom: Log tabs always started with a static line like `rusty-kaspa:v1.1.0-rc.3`, which added noise and was not a runtime log event.
- Root cause: Services pre-populated their in-memory log buffers with a hardcoded version message.
- Fix: Initialize log buffers as empty (`Vec::new()`) for:
  - Rusty Kaspa logs
  - RK Bridge logs
  - CPU Miner logs
  - Rothschild logs
- Files touched: `core/src/runtime/services/kaspa/mod.rs`, `core/src/runtime/services/stratum_bridge.rs`, `core/src/runtime/services/cpu_miner.rs`, `core/src/runtime/services/rothschild.rs`.

## 2026-02-22 - Prevent TN12 startup panic in local consensus/wallet params lookup
- Symptom: Switching to Testnet 12 with local node caused panic: `Testnet suffix 12 is not supported`.
- Root cause: Rusty-Kaspa parameter mapping only recognized testnet suffix `10` in:
  - consensus params conversion (`From<NetworkId> for Params`)
  - wallet network params lookup.
- Fix: Add suffix `12` handling and map it to existing testnet parameter sets (same as suffix `10`) to avoid crash paths.
- Files touched: `rusty-kaspa/consensus/core/src/config/params.rs`, `rusty-kaspa/wallet/core/src/utxo/settings.rs`.

## 2026-02-22 - Hide/disable RK Bridge on testnet networks
- Symptom: RK Bridge was still visible/selectable while using testnet networks.
- Root cause: Menu and settings gating only checked "local node", not active network.
- Fix:
  - Hide `RK Bridge` tab when network is not `Mainnet`.
  - Disable RK Bridge settings controls on testnet and show explanatory message.
  - Auto-disable RK Bridge when switching away from Mainnet (both quick network change path and Settings apply path).
- Files touched: `core/src/menu.rs`, `core/src/core.rs`, `core/src/modules/settings/mod.rs`.

## 2026-02-22 - Align build pipeline with in-repo `cpuminer` path
- Symptom: Build script failed after moving `cpuminer/` into the main repo root (`current package believes it's in a workspace when it's not`).
- Root cause: root workspace did not explicitly exclude `cpuminer`, while `core/build.rs` invokes `cargo build --release` inside `cpuminer`.
- Fix:
  - Added `exclude = ["cpuminer"]` to root workspace manifest.
  - Extended `core/build.rs` to build/copy `kaspa-miner` and `rothschild` during normal app build (same target profile output directory as app binary).
- Files touched: `Cargo.toml`, `core/build.rs`.

## 2026-02-22 - Avoid TN12↔TN10 runtime network mismatch in wallet connect flow
- Symptom: Runtime logged `Invalid network type - expected: testnet-12 connected to: testnet-10`.
- Root cause: UI/network model exposed `Testnet12`, but current node/runtime stack still resolves effective testnet identity to TN10 in this branch.
- Fix: Mapped `Network::Testnet12` to testnet suffix `10` when converting to `NetworkId` (compat mode), so wallet/network handshake no longer mismatches.
- Files touched: `core/src/network.rs`.

## 2026-02-22 - Accept TN10 account stream while UI is set to TN12 (compat mode)
- Symptom: In TN12 mode, wallet runtime emitted `error processing wallet runtime event: Invalid network 'testnet-10'`.
- Root cause: Account loading path compared selected network (`Testnet12`) strictly against server-reported network (`Testnet10`) and rejected it.
- Fix: In account loading, treat `(selected=Testnet12, actual=Testnet10)` as an allowed effective match while TN12 compatibility mapping is active.
- Files touched: `core/src/core.rs`.

## 2026-02-22 - Explorer now applies per-network official/self-hosted API endpoints
- Symptom: Explorer UI continued calling hardcoded endpoints even after selecting different official/self-hosted network profiles in app settings.
- Root cause:
  - frontend had hardcoded API/socket constants;
  - embedded WebView did not inject runtime endpoint config from `Settings.explorer`.
- Fix:
  - Added runtime frontend config (`__KASPA_EXPLORER_CONFIG__`) support in explorer app (`app/api/config.ts`) and switched API/socket consumers to this config.
  - Embedded WebView now injects endpoint config from `core.settings.explorer.endpoint(core.settings.node.network)` at startup.
  - WebView is recreated when explorer endpoint/network/source changes, so new API/socket paths take effect immediately.
- Files touched: `core/src/modules/explorer.rs`, `kaspa-explorer-ng/app/api/config.ts`, `kaspa-explorer-ng/app/api/socket.ts`, `kaspa-explorer-ng/app/api/kaspa-api-client.ts`, multiple files in `kaspa-explorer-ng/app/hooks/`, `kaspa-explorer-ng/app/routes/miners.tsx`.

## 2026-02-22 - Fix Windows typing in Wallet unlock password input
- Symptom: On Windows, the `Unlock Wallet` password input (`Enter the password to unlock your wallet`) became active and immediately lost focus, so typing was impossible.
- Root cause: `WalletOpen` used direct `TextEdit` + manual `request_focus`, not the shared `TextEditor`/`FocusManager` flow that already contains the Windows focus-steal fix.
- Fix:
  - Migrated unlock password field to `TextEditor` + `FocusManager`.
  - Replaced `focus_unlock_editor: bool` with typed focus state.
  - Kept submit-on-Enter behavior through `TextEditor::submit`.
- Consistency check:
  - Wallet module inputs in `wallet_open`, `wallet_create`, `wallet_secret`, and `account_manager/*` all now use `TextEditor` focus-managed flow.
- Files touched: `core/src/modules/wallet_open.rs`.

## 2026-02-22 - Save Explorer wallet address per network (Mainnet/TN10/TN12)
- Symptom: `Save as my wallet address` stored one global address, causing invalid address usage after switching networks (e.g. Mainnet address shown on TN12).
- Root cause: Storage key was static (`kaspaExplorerSavedAddress`) and not network-scoped.
- Fix:
  - Added network-scoped key helper in explorer frontend storage.
  - Address save/load now uses `savedAddressKeyForNetwork(networkId)`.
  - Embedded WebView injects `networkId` into runtime explorer config (`__KASPA_EXPLORER_CONFIG__`).
- Files touched: `kaspa-explorer-ng/app/utils/storage.ts`, `kaspa-explorer-ng/app/Dashboard.tsx`, `kaspa-explorer-ng/app/routes/addressdetails.tsx`, `kaspa-explorer-ng/app/api/config.ts`, `core/src/modules/explorer.rs`.

## 2026-02-22 - Rothschild auto-generates key/address + mnemonic when empty
- Symptom: Rothschild logged `private key is not set (configure it in Settings)` when enabled with empty key, and mnemonic support from TN12 branch was missing in main flow.
- Root cause: Main settings/runtime path allowed Rothschild to start without generating credentials, and mnemonic backfill was not consistently applied.
- Fix:
  - Added TN12-style auto-generation in Settings when Rothschild is enabled and key is empty.
  - On manual `Private Key` edits, mnemonic + address are now re-derived immediately from the new key (instead of only when fields were empty).
  - Added mnemonic derivation from private key and read-only mnemonic display with copy support.
  - Added load-time migration/backfill for enabled Rothschild: generate key/address (TN10/TN12), derive mnemonic, and seed CPU miner address if empty.
  - Added shared Rothschild utility exports and fixed network prefix mapping so both `Testnet10` and `Testnet12` use testnet addressing.
  - Added missing `secp256k1` dependency to `core` crate.
- Files touched: `core/src/modules/settings/mod.rs`, `core/src/settings.rs`, `core/src/utils/mod.rs`, `core/src/utils/rothschild.rs`, `core/Cargo.toml`.

## 2026-02-22 - Add Explorer Official/Self-hosted settings UI with per-network endpoints
- Symptom: Explorer endpoint model existed in settings, but there was no UI to switch data source or edit per-network official/self-hosted API/socket endpoints.
- Root cause: `ExplorerSettings` was wired in runtime/WebView injection, but not exposed in Settings module.
- Fix:
  - Added `Explorer API` section in Settings.
  - Added data-source switch (`Official` / `Self-hosted`) persisted in settings.
  - Added editable endpoint profiles for `Mainnet`, `Testnet10`, `Testnet12` under both official and self-hosted groups (`api_base`, `socket_url`, `socket_path`).
  - Added active-network endpoint preview so currently effective API/socket is visible.
  - Existing Explorer module reload trigger remains in place and applies changes because signature includes source+network+endpoint fields.
- Files touched: `core/src/modules/settings/mod.rs`.

## 2026-02-22 - Wire Self-hosted runtime services to Explorer data source
- Symptom: Self-hosted service modules existed in the repo but were not active in runtime and not toggled when switching Explorer source.
- Root cause: Missing integration in runtime/service registration, settings model lacked `SelfHostedSettings`, and no source-change hook enabled/disabled services.
- Fix:
  - Reintroduced `SelfHostedSettings` into app settings with defaults + migration/backfill (`db_user`, `db_name`, generated DB password, ports).
  - Added runtime service registration/getters for:
    - `self-hosted-db`
    - `self-hosted-indexer`
    - `self-hosted-postgres`
    - `self-hosted-explorer`
  - Added settings-side orchestration: when Explorer source switches to `Self-hosted`, services are enabled and updated; when switched back to `Official`, they are disabled.
  - Added full Self-hosted settings UI (API/DB/indexer/postgres fields) in Services.
  - DB password behavior: empty/default (`kaspa`) is auto-replaced with a random generated password, and a manual `Regenerate` action is available.
  - Restored explicit Self-hosted enable/disable checkbox in Services (independent from Explorer source switch).
  - Removed `Copy` actions for `Database User` and `Database Name`.
  - Changed `Database Password` copy action to icon button style (same visual pattern as Rothschild settings).
  - Added node-settings propagation to self-hosted explorer service (`update_node_settings`) when node config is applied.
  - Added `Database` module tab visibility when self-hosted is enabled.
  - Added missing dependencies for self-hosted services (`axum`, `tokio-postgres`, `tokio-stream`).
  - Updated self-hosted explorer network mapping to handle `Testnet12`.
- Files touched: `core/src/settings.rs`, `core/src/imports.rs`, `core/src/runtime/services/mod.rs`, `core/src/runtime/mod.rs`, `core/src/runtime/services/self_hosted_explorer.rs`, `core/src/modules/settings/mod.rs`, `core/src/modules/mod.rs`, `core/src/menu.rs`, `core/Cargo.toml`, `Cargo.toml`.

## 2026-02-22 - Add simply-kaspa-indexer to build pipeline + update build README flow
- Symptom: `simply-kaspa-indexer` was required for self-hosted mode but not included in the main build pipeline.
- Root cause: `core/build.rs` only built explorer/cpuminer/rothschild/stratum-bridge.
- Fix:
  - Added `build_simply_kaspa_indexer_if_needed()` to `core/build.rs` with change detection and release build (`cargo build -p simply-kaspa-indexer --release`).
  - Added binary sync to app target profile directory (`target/<profile>/simply-kaspa-indexer[.exe]`).
  - Updated root `README.md` build steps to include:
    - `simply-kaspa-indexer` release build
    - `kaspa-rest-server` Poetry install
    - `kaspa-socket-server` Pipenv install
    - explorer `npm build` before app release build.
- Files touched: `core/build.rs`, `README.md`.

## 2026-02-22 - Reduce self-hosted startup errors (port-in-use + DB readiness)
- Symptom: Enabling self-hosted on Mainnet produced noisy startup errors:
  - REST/socket repeatedly failed with `Connection in use` on ports `8000` / `8001`.
  - Early Postgres `database "kaspa" does not exist` errors during initialization race.
- Root cause:
  - REST/socket services attempted to start even when selected ports were already occupied by external processes.
  - Indexer DB readiness check connected directly to target DB before creation completed.
- Fix:
  - Added port availability pre-checks in self-hosted explorer service; if a port is occupied, startup is skipped with a clear warning instead of repeated gunicorn retries.
  - Updated indexer DB wait flow:
    - connect to `postgres` first,
    - poll for target DB existence,
    - connect to target DB only after it exists.
  - Adjusted self-hosted service enable order in Settings to start Postgres/Indexer before DB/Explorer API toggles.
- Files touched: `core/src/runtime/services/self_hosted_explorer.rs`, `core/src/runtime/services/self_hosted_indexer.rs`, `core/src/modules/settings/mod.rs`.

## 2026-02-22 - Add network-scoped reset button in Settings
- Symptom: Needed a fast way to clear settings related to the currently selected network without wiping all app settings.
- Root cause: Existing Settings UI only had global `Reset Settings`.
- Fix:
  - Added `Reset Current Network Settings` button with confirmation in Settings.
  - Reset logic is network-aware:
    - Mainnet: resets RK Bridge settings/state + explorer endpoints for Mainnet.
    - Testnet10/Testnet12: resets CPU Miner + Rothschild settings/state + explorer endpoints for selected testnet.
  - Applies runtime service updates immediately and persists settings.
- Files touched: `core/src/modules/settings/mod.rs`.

## 2026-02-22 - Self-hosted startup log-noise reduction on Mainnet
- Symptom: Enabling self-hosted produced heavy startup noise:
  - repeated readiness failures while Postgres recovery was still in progress,
  - noisy warnings for occupied REST/socket ports even when external services are intentionally running.
- Root cause:
  - Postgres readiness check attempted multiple connection strategies every 500ms, amplifying transient recovery logs.
  - Occupied explorer ports were treated as warnings rather than expected external-service mode.
- Fix:
  - Postgres readiness probing now performs reduced fallback attempts and uses a 1s interval.
  - Occupied REST/socket ports are now logged as info (`assuming external ... server`) instead of warnings.
  - Existing DB existence race fix for indexer remains active.
- Files touched: `core/src/runtime/services/self_hosted_postgres.rs`, `core/src/runtime/services/self_hosted_explorer.rs`.

## 2026-02-22 - Normalize self-hosted REST/socket log levels from gunicorn output
- Symptom: REST/socket logs showed many `[WARN]` entries even for normal startup info lines (`[INFO] Starting gunicorn`, `Application startup complete`).
- Root cause: stderr stream was mapped to `WARN` unconditionally, while gunicorn writes informational logs to stderr.
- Fix:
  - Added line-based level detection (`INFO` / `WARN` / `ERROR`) for REST/socket child-process output.
  - Both stdout/stderr lines are now classified by content before writing to log store.
- Files touched: `core/src/runtime/services/self_hosted_explorer.rs`.

## 2026-02-22 - Normalize self-hosted Postgres log levels + reduce checkpoint warning spam
- Symptom: Postgres log view showed many normal startup/runtime lines as `[WARN]` and frequent checkpoint warnings (`Checkpoints passieren zu oft`).
- Root cause:
  - stderr stream from postgres was classified as `WARN` unconditionally.
  - default postgres checkpoint/WAL settings were too conservative for sustained indexer writes.
- Fix:
  - Added content-based Postgres level mapping (`LOG`/`HINT`/`TIPP` => `INFO`, `WARNING`/`WARNUNG` => `WARN`, `ERROR`/`FEHLER`/`FATAL` => `ERROR`).
  - Added postgres runtime tuning args:
    - `max_wal_size=4GB`
    - `checkpoint_timeout=15min`
    - `checkpoint_completion_target=0.9`
- Files touched: `core/src/runtime/services/self_hosted_postgres.rs`.

## 2026-02-22 - Force self-hosted Postgres/initdb logs to English
- Symptom: Postgres/initdb output appeared in system locale (e.g. German) in app logs.
- Root cause: child processes inherited host locale defaults.
- Fix:
  - Set `LC_MESSAGES=C` for `initdb` and `psql` subprocesses.
  - Start postgres with `-c lc_messages=C` and environment `LANG=C`, `LC_ALL=C`, `LC_MESSAGES=C`.
- Files touched: `core/src/runtime/services/self_hosted_postgres.rs`.

## 2026-02-22 - Downgrade transient Postgres recovery connection errors in logs
- Symptom: During startup recovery, Postgres emitted temporary `not yet accepting connections` / `consistent recovery state` lines that appeared as errors/warnings in UI logs.
- Root cause: These transient recovery messages were classified with high severity by log-level detection.
- Fix:
  - In Postgres log classification, map those specific startup-recovery messages to `INFO`.
- Files touched: `core/src/runtime/services/self_hosted_postgres.rs`.

## 2026-02-22 - Harden self-hosted service start semantics (no duplicate starts, strict port handling)
- Symptom: Port-in-use situations appeared even when services should be started only by Kaspa NG.
- Root cause:
  - repeated enable events could trigger duplicate start attempts while already running,
  - services lacked strong "already running" guards,
  - some listeners (DB/indexer) had no pre-bind conflict checks.
- Fix:
  - Added idempotent enable/disable handling for self-hosted services (`db`, `indexer`, `postgres`, `explorer`): enable/disable now no-op when state is unchanged.
  - Added running-instance guards in service start paths (`rest`, `socket`, `db`, `indexer`) to avoid double-spawn.
  - `self-hosted-db`: explicit address-in-use error reporting at bind.
  - `self-hosted-indexer`: pre-check `indexer_listen` address and refuse start with explicit error if occupied.
  - Settings flow no longer re-sends enable toggles on every field edit; enable calls are sent only when enabled-state actually changes.
  - Explorer REST/socket keep strict port checks and now refuse start when occupied.
- Files touched: `core/src/runtime/services/self_hosted_explorer.rs`, `core/src/runtime/services/self_hosted_postgres.rs`, `core/src/runtime/services/self_hosted_indexer.rs`, `core/src/runtime/services/self_hosted_db.rs`, `core/src/modules/settings/mod.rs`.

## 2026-02-22 - Ensure hidden self-hosted indexer/postgres toggles cannot disable startup
- Symptom: Indexer could remain stopped with empty log output after settings cleanup removed `Indexer Enabled` / `Postgres Enabled` controls.
- Root cause: persisted legacy values (`indexer_enabled=false` / `postgres_enabled=false`) could still exist and block startup, but no longer be editable in UI.
- Fix:
  - Settings migration now force-enables `self_hosted.indexer_enabled` and `self_hosted.postgres_enabled`.
  - Settings save path in UI also enforces both values to `true`.
  - Indexer binary lookup now ignores non-existent custom path and falls back to default locations.
  - Indexer startup failures now also write explicit entries to indexer log store.
- Files touched: `core/src/settings.rs`, `core/src/modules/settings/mod.rs`, `core/src/runtime/services/self_hosted_indexer.rs`.

## 2026-02-22 - Change self-hosted default REST/socket ports to 19112/19113
- Symptom: Needed non-conflicting default ports for local self-hosted REST/socket services.
- Fix:
  - Changed default self-hosted service ports:
    - REST: `19112`
    - Socket: `19113`
  - Updated default self-hosted explorer endpoints to `http://127.0.0.1:19112` and `http://127.0.0.1:19113`.
  - Added migration from legacy defaults (`8000`/`8001`) for both service ports and unchanged self-hosted explorer endpoint profiles.
- Files touched: `core/src/settings.rs`.

## 2026-02-22 - Ensure REST/socket child processes are terminated with kaspa-ng shutdown
- Symptom: After closing `kaspa-ng`, `kaspa-socket-server` (and sometimes REST) could remain running, leaving ports occupied for the next launch.
- Root cause:
  - killing only the tracked parent process was not always sufficient for gunicorn worker trees,
  - strict `unwrap()` in service `terminate()` could panic during shutdown if a receiver was already closed.
- Fix:
  - On Unix, REST/socket are now spawned in their own process group (`process_group(0)`).
  - On shutdown, service now terminates the whole process group (`SIGTERM`, then `SIGKILL` fallback with timeout).
  - Made `terminate()` for self-hosted services resilient by replacing `try_send(...).unwrap()` with non-panicking `let _ = try_send(...)`.
- Verification:
  - Started `kaspa-ng`, then sent `SIGTERM`.
  - Confirmed no leftover listeners/processes on ports `19112`, `19113`, `8500`, `5432`.
- Files touched: `core/src/runtime/services/self_hosted_explorer.rs`, `core/src/runtime/services/self_hosted_postgres.rs`, `core/src/runtime/services/self_hosted_indexer.rs`, `core/src/runtime/services/self_hosted_db.rs`.

## 2026-02-22 - Auto-switch Explorer to Self-hosted only after full service readiness
- Requirement: Enabling `Self Hosted` should switch Explorer automatically, but only after services are actually reachable to avoid follow-up errors.
- Fix:
  - Added startup readiness check in Settings flow when `Self Hosted` is toggled ON.
  - Auto-switch to `ExplorerDataSource::SelfHosted` now happens only if all of these accept TCP connections:
    - Postgres (`db_host:db_port`)
    - Indexer API (`api_bind:api_port`)
    - Explorer REST (`api_bind:explorer_rest_port`)
    - Explorer Socket (`api_bind:explorer_socket_port`)
  - If readiness is not reached within timeout, Explorer remains/returns to `Official` and a warning toast is shown.
  - When `Self Hosted` is toggled OFF and Explorer source is `SelfHosted`, it is switched back to `Official` automatically.
- Notes:
  - `api_bind` wildcard values (`0.0.0.0` / `::`) are probed via `127.0.0.1` for local readiness checks.
- File touched: `core/src/modules/settings/mod.rs`.

## 2026-02-22 - Integrate K + K-indexer into main kaspa-ng project
- Goal: remove dependency on external side folders and make K/K-indexer part of regular build/runtime flow.
- Build integration (`core/build.rs`):
  - Auto-sync repositories in workspace root on build:
    - `K` from `https://github.com/thesheepcat/K.git`
    - `K-indexer` from `https://github.com/thesheepcat/K-indexer.git`
  - Auto-build K frontend (`npm install` if needed + `npm run build`) and sync `K/dist` into `target/<profile>/K/dist`.
  - Auto-build K-indexer binaries (`K-webserver`, `K-transaction-processor`) and sync binaries into `target/<profile>/`.
  - Added optional fast-check bypass env var: `KASPA_NG_SKIP_EXTERNAL_BUILDS=1` (does not change default behavior).

## 2026-02-22 - Add K-indexer runtime service and DB/log integration
- Added new native service: `SelfHostedKIndexerService` (`core/src/runtime/services/self_hosted_k_indexer.rs`).
- Service behavior:
  - Starts/stops `K-transaction-processor` and `K-webserver` as child processes.
  - Uses same self-hosted Postgres credentials/database as existing stack.
  - Uses selected node network mapping:
    - mainnet -> `mainnet`
    - testnet10 -> `testnet-10`
    - testnet12 -> not supported (logs warning and does not start).
  - `K-webserver` bind port uses new self-hosted setting `k_web_port` (default `3000`).
- Runtime wiring:
  - Added new log store channel `k_indexer`.
  - Added service to runtime startup/shutdown and settings update pathways.
- Self-hosted DB API:
  - Added logs endpoint source `k-indexer` via `/api/logs/k-indexer`.
- Database UI:
  - Added `K-indexer` selectable log source.
  - Added K tables to `Indexer Tables` section:
    - `k_vars`, `k_broadcasts`, `k_votes`, `k_mentions`, `k_blocks`, `k_follows`, `k_contents`, `k_hashtags`.

## 2026-02-22 - Add K-Social WebView tab with settings toggle
- Added new native module/tab: `K-Social` (`core/src/modules/k_social.rs`).
- Module behavior:
  - Hosts built `K` frontend via local static server (similar embedded web approach as Explorer).
  - Injects runtime config into localStorage (`kaspa_user_settings`) so K uses local `K-webserver` URL (`http://<api_bind>:<k_web_port>`) and active network.
- Menu/tab visibility:
  - `K-Social` tab shown only when:
    - `self_hosted.enabled == true`
    - `self_hosted.k_enabled == true`
- Settings:
  - Added `Enable K-Social services` toggle.
  - Added `K API Port` setting field (default `3000`).
  - `k_enabled` defaults to `false`.

## 2026-02-22 - Improve K-Social readiness diagnostics and avoid endless loading
- Problem: K-Social could appear to load forever without clear UI state when backend API was not fully ready.
- Fixes in `core/src/modules/k_social.rs`:
  - Replaced plain TCP probe with HTTP `/health` probe and status parsing.
  - Added explicit K-indexer API status line in the K-Social tab header (ready/not ready + HTTP state).
  - Added waiting timer display while K-indexer is initializing.
  - Added non-fatal initialization info after prolonged wait (`>=30s`) instead of ambiguous blank loading.
  - Improved log signal by recording API reachability transitions and preserving existing WebView attach/error logs.

## 2026-02-22 - Keep K private-key fix stable across git auto-updates
- Problem: `K` is git-synced during build, so direct edits in `K/src/...` can be overwritten by `git pull`.
- Fix in `core/build.rs`:
  - Added auto-stash/unstash around external repo pulls when local changes exist (for `K` and other synced repos).
  - Added idempotent compatibility patch application for:
    - `K/src/services/kaspaService.ts`
    - `K/src/contexts/AuthContext.tsx`
  - Patch is applied before K build and survives upstream updates without manual re-editing.

## 2026-02-22 - Remove remaining terminal control bytes from UI log rendering
- Symptom: sporadic `epaint` warning about missing replacement glyphs (`◻` / `?`) while services were running.
- Root cause: some subprocess outputs include non-printable terminal control sequences that were not fully removed by the simple ANSI stripper.
- Fix:
  - Hardened `strip_ansi_codes` in `core/src/runtime/services/log_store.rs` to handle CSI/OSC/DCS/SOS/PM/APC escape families.
  - Dropped non-printable control chars (except `\\n`, `\\r`, `\\t`) before storing UI log lines.

## 2026-02-22 - Ensure K-Social API status and logs are always visible in UI
- Symptom: `K-Social` log panel and API status message could disappear when waiting/not-ready states were active.
- Fix in `core/src/modules/k_social.rs`:
  - Reordered render flow so API status and log panel are rendered before WebView allocation.
  - Removed early returns that skipped the log panel in waiting/disabled states.
  - WebView now attaches only after status/log section is shown and API is ready.

## 2026-02-22 - K-Social UI: remove log panel, add Hard Reset next to API readiness
- Requirement:
  - Remove `K-Social Logs` section from the tab.
  - Keep API readiness label (e.g. `K-indexer API: ready (HTTP/1.1 200 OK)`).
  - Add a `Hard Reset` button near API status to recover from stuck loading.
- Implementation in `core/src/modules/k_social.rs`:
  - Removed log panel rendering in UI.
  - Added `Hard Reset` button beside API status (or standalone if status not yet present).
  - `Hard Reset` now force-reinitializes K-Social embed by dropping/recreating:
    - WebView
    - local static server
    - probe/signature/bounds state

## 2026-02-22 - K-Social hard reset stability + deeper K serve diagnostics
- Fixed `Hard Reset` regression where `K-Social` could stay blank after reset:
  - Added lazy local server re-init in render (`ensure_local_server`) so WebView can reattach after reset.
  - Made reset execution deferred (`pending_hard_reset`) to avoid unsafe in-frame destruction side effects.
- Added deeper direct serve diagnostics:
  - K static server now logs every request with method/path/status/content-type (`k-social-server: ...`) to runtime logs.
  - Added `K Web Server: ...` HTTP readiness line in K-Social UI (`GET /` probe) alongside `K-indexer API: ...`.

## 2026-02-22 - Explorer price visibility fix: MarketDataProvider wiring restored
- Root cause: `MarketDataProvider` was not wrapped in explorer root layout, so Coingecko market data context stayed undefined in UI.
- Fix: re-added `<MarketDataProvider>` wrapper in `kaspa-explorer-ng/app/root.tsx` around header/content/footer.
- Follow-up fix:
  - Re-added missing `<Price />` component mount in `kaspa-explorer-ng/app/header/Header.tsx` (was present in `kaspa-explorer-ng-main` only).
  - Without this, market-data was fetched but never rendered in header.

## 2026-02-22 - Test mode: revert K private-key patch
- Reverted testwise changes in:
  - `K/src/services/kaspaService.ts`
  - `K/src/contexts/AuthContext.tsx`
- Disabled automatic re-application during build by default:
  - `core/build.rs` now applies the K private-key patch only when `KASPA_NG_ENABLE_K_PRIVATE_KEY_PATCH=1`.

## 2026-02-22 - Self-hosted shutdown order: terminate Postgres last
- Symptom: On app shutdown, `simply-kaspa-indexer` could panic because Postgres was terminated while indexer workers were still writing.
- Fix in `core/src/runtime/mod.rs`:
  - `Runtime::stop_services()` now terminates all services except `self-hosted-postgres` first.
  - Added a short grace delay (`750ms`) before sending terminate to `self-hosted-postgres`.
  - Postgres shutdown is now explicitly last in service termination order.

## 2026-02-22 - Prevent Ctrl+C from signaling DB/indexers directly (unix process groups)
- Symptom: subprocesses (`simply-kaspa-indexer`, `K-*`, postgres) could receive terminal `SIGINT` directly, bypassing runtime shutdown sequencing.
- Fix:
  - Spawned these services in separate process groups on Unix (`process_group(0)`):
    - `core/src/runtime/services/self_hosted_postgres.rs`
    - `core/src/runtime/services/self_hosted_indexer.rs`
    - `core/src/runtime/services/self_hosted_k_indexer.rs`
- Result:
  - Shutdown now follows kaspa-ng service order reliably.
  - Verified no indexer panic on stop; postgres now logs smart shutdown after dependent services stop.

## 2026-02-22 - Restore Explorer CoinGecko price flow and USD value under KAS amount
- Reference checked against `kaspa-explorer-ng-main`.
- Fixes in `kaspa-explorer-ng`:
  - Re-added missing `getMarketData()` export in `app/api/kaspa-api-client.ts` (`/info/market-data`).
  - Hardened `app/context/MarketDataProvider.tsx` with fallback handling if market-data is unavailable (e.g. non-mainnet/self-hosted modes).
  - Restored fiat conversion display in transaction details:
    - `app/routes/transactiondetails.tsx` now shows USD value under the KAS transfer amount via `MarketDataContext`.
  - Restored fiat conversion display in address details balance block:
    - `app/routes/addressdetails.tsx` now shows USD value under KAS balance via `MarketDataContext`.
