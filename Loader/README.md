# Loader

This folder documents the self-hosted Database Loader service.

The runtime implementation lives in:
- `core/src/runtime/services/self_hosted_loader.rs`

The Loader is a dedicated service that orchestrates startup order for self-hosted Database mode:
1. Postgres
2. Node sync (wait until the node is synced)
3. Indexers
4. REST API + Socket server

It also performs periodic health pings, restarts unhealthy services with cooldowns, and updates loader status for the Database UI.

## Build

Build the app/runtime services with:

```bash
cargo build -p kaspa-ng-core
```
