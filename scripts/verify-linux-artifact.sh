#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="${1:?Usage: $0 <artifact-root>}"

required_bins=(kaspa-ng stratum-bridge simply-kaspa-indexer K-webserver K-transaction-processor kasia-indexer)
required_dirs=(kaspa-explorer-ng kaspa-rest-server kaspa-socket-server Loader)

for f in "${required_bins[@]}"; do
  [ -x "$ROOT/$f" ] || { echo "Missing packaged executable: $f" >&2; exit 1; }
done

for d in "${required_dirs[@]}"; do
  [ -d "$ROOT/$d" ] || { echo "Missing packaged directory: $d" >&2; exit 1; }
done

[ -d "$ROOT/postgres" ] || { echo "Missing packaged directory: postgres" >&2; exit 1; }

for pgbin in postgres initdb pg_ctl; do
  [ -x "$ROOT/postgres/bin/$pgbin" ] || { echo "Missing PostgreSQL runtime binary: postgres/bin/$pgbin" >&2; exit 1; }
done

ld_path="$ROOT/postgres/lib"
if [ -d "$ROOT/postgres/lib64" ]; then
  ld_path="$ld_path:$ROOT/postgres/lib64"
fi

for pgbin in postgres initdb pg_ctl; do
  if ! LD_LIBRARY_PATH="$ld_path${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" "$ROOT/postgres/bin/$pgbin" --version >/dev/null 2>&1; then
    echo "PostgreSQL runtime binary is not runnable: postgres/bin/$pgbin" >&2
    exit 1
  fi
done

if ldd "$ROOT/postgres/bin/postgres" 2>/dev/null | grep -q "not found"; then
  echo "PostgreSQL runtime has unresolved shared libraries in artifact" >&2
  ldd "$ROOT/postgres/bin/postgres" 2>/dev/null || true
  exit 1
fi

bash "$(dirname "$0")/verify-self-hosted-python-runtime.sh" "$ROOT"

echo "Linux artifact verification passed: $ROOT"
