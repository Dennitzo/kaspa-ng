# Web UI + Lokaler Companion-Service

## Ziel
- `trunk serve` liefert nur die UI (WASM).
- Ein lokaler Companion-Service startet/stoppt native Dienste (kaspad, postgres, indexer, explorer, miner, rothschild) und stellt sie der Web-UI per HTTP/WebSocket bereit.
- Feature-Parität zu Desktop-Native ohne Browser-Workarounds.

## Architektur
- `kaspa-ng-web` (bestehend): UI in Browser.
- `kaspa-ng-companion` (neu): lokaler Prozess auf `127.0.0.1`, kontrolliert Runtime/Services.
- Kommunikation:
  - REST für Commands/Status.
  - WebSocket für Logs + Live-Events.

## Warum so?
- Browser kann keine lokalen Binaries spawnen und keine Prozesse killen.
- Companion kapselt genau die Pfade, die heute in `core/src/runtime/services/*` nativ laufen.

## Service-Mapping (bestehende Module)
- Node:
  - `core/src/runtime/services/kaspa/*`
- Self-Hosted:
  - `core/src/runtime/services/self_hosted_postgres.rs`
  - `core/src/runtime/services/self_hosted_indexer.rs`
  - `core/src/runtime/services/self_hosted_explorer.rs`
  - `core/src/runtime/services/self_hosted_k_indexer.rs`
- Mining:
  - `core/src/runtime/services/cpu_miner.rs`
  - `core/src/runtime/services/rothschild.rs`
- Runtime-Koordination:
  - `core/src/runtime/mod.rs`

## API-Vertrag (V1)

### 1) Health
- `GET /api/v1/health`
- Response:
```json
{
  "ok": true,
  "version": "1.1.0-rc.3",
  "companion_uptime_sec": 123
}
```

### 2) Gesamtstatus
- `GET /api/v1/status`
- Response:
```json
{
  "network": "mainnet",
  "services": {
    "kaspad": "running",
    "postgres": "running",
    "indexer": "stopped",
    "explorer": "running",
    "k_indexer": "running",
    "cpu_miner": "stopped",
    "rothschild": "stopped"
  },
  "ports": {
    "rpc": 16210,
    "postgres": 5432,
    "indexer": 8500,
    "explorer_rest": 17110,
    "explorer_socket": 17111
  }
}
```

### 3) Netzwerkwechsel
- `POST /api/v1/network/switch`
- Request:
```json
{ "network": "testnet-12" }
```
- Semantik:
  - Dienste geordnet stoppen.
  - Settings für Zielnetzwerk laden (oder defaults).
  - Dienste für Zielnetzwerk starten.
  - Bei bereits belegtem Netzwerk: `409 Network already in use`.

### 4) Service steuern
- `POST /api/v1/services/{name}/start`
- `POST /api/v1/services/{name}/stop`
- `POST /api/v1/services/{name}/restart`
- `{name}`: `kaspad|postgres|indexer|explorer|k_indexer|cpu_miner|rothschild`

### 5) Settings
- `GET /api/v1/settings/{network}`
- `PUT /api/v1/settings/{network}`
- Regeln:
  - Dateinamen nur netzwerkbasiert:
    - `kaspa-ng.mainnet.settings`
    - `kaspa-ng.tn10.settings`
    - `kaspa-ng.tn12.settings`
  - Fehlende Datei => defaults + Port-Offset.

### 6) Logs streamen
- `GET /api/v1/logs/ws?stream=all|postgres|indexer|kaspad|explorer|cpu_miner|rothschild`
- WebSocket Events:
```json
{
  "ts": "2026-02-25T16:00:00Z",
  "stream": "postgres",
  "level": "INFO",
  "msg": "database system is ready to accept connections"
}
```

## Sicherheit
- Companion nur `127.0.0.1` binden.
- Pro Instanz ein zufälliges Session-Token (Datei in `~/.kaspa/companion/token`).
- Jeder mutierende Endpoint verlangt `Authorization: Bearer <token>`.
- Optional: CORS nur für `https://localhost:*`.

## Netzwerk- und Port-Konsistenz
- Portberechnung zentral aus `network` + `base_port` (keine ad-hoc Offsets).
- Bei Switch immer:
  - `stop_services()` + Join/Timeout.
  - erst danach neue Ports binden.
- Wenn gleicher Netzwerk-Lock existiert:
  - UI-Warnung: `Network already in use`.
  - kein stilles Fallback auf Random-Port.

## Umsetzung in 4 Phasen

### Phase 1: Companion-Prozess (MVP)
- Neues Binary `kaspa-ng-companion` (axum + tokio).
- Endpoints: `health`, `status`, `network/switch`.
- In-Memory Service State + WebSocket Log-Broadcast.

### Phase 2: Runtime entkoppeln
- Aus `core/src/runtime/mod.rs` eine wiederverwendbare `Orchestrator`-Schicht extrahieren:
  - `start_all(network, settings)`
  - `stop_all()`
  - `restart(service)`
  - `service_state()`
- Desktop-App nutzt weiterhin dieselbe Orchestrator-Logik.
- Companion nutzt dieselbe Orchestrator-Logik.

### Phase 3: Web-UI Adapter
- Neue `CoreBridge` Implementierung für Web:
  - statt direkter Native-Aufrufe -> HTTP/WS gegen Companion.
- UI-Module bleiben weitgehend gleich, Datenquelle wird austauschbar:
  - Native: lokal
  - Web: Companion API

### Phase 4: Packaging
- macOS:
  - `Kaspa-NG.app` startet beim Launch den Companion (falls nicht aktiv).
  - Beim App-Exit Companion sauber stoppen (oder optional persistent lassen).
- Dev:
  - Terminal 1: `kaspa-ng-companion`
  - Terminal 2: `trunk serve --release --port 8080`

## Konkrete Dateistruktur (Vorschlag)
- `companion/Cargo.toml`
- `companion/src/main.rs`
- `companion/src/api/{health,status,services,network,settings,logs}.rs`
- `core/src/orchestrator/*` (aus Runtime extrahiert)
- `core/src/bridge/{native,web_companion}.rs`

## Akzeptanzkriterien
- Web-UI kann:
  - Netzwerk wechseln ohne Port-Leaks.
  - Dienste starten/stoppen/restarten.
  - Live-Logs sehen.
  - self-hosted Explorer/Indexer/Postgres stabil nutzen.
- Keine `Address already in use` Fehler nach sauberem Switch.
- Keine Abhängigkeit von Browser-Privilegien für native Prozesse.

## Minimaler Startbefehl (später)
```bash
# 1) Companion lokal starten
cargo run -p kaspa-ng-companion --release

# 2) Web UI starten
trunk serve --release --port 8080
```

