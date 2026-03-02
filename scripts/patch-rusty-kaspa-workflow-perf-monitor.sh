#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="${1:-${ROOT_DIR}/rusty-kaspa/Cargo.toml}"
PATCH_LINE='workflow-perf-monitor = { path = "../vendor/workflow-perf-monitor" }'

if [ ! -f "$CARGO_TOML" ]; then
  echo "[rusty-kaspa-patch] skipped: file not found: $CARGO_TOML"
  exit 0
fi

if grep -Fq "$PATCH_LINE" "$CARGO_TOML"; then
  echo "[rusty-kaspa-patch] already present in $CARGO_TOML"
  exit 0
fi

TMP_FILE="$(mktemp)"
awk -v patch_line="$PATCH_LINE" '
BEGIN { added = 0; in_patch = 0 }
{
  if ($0 ~ /^\[patch\.crates-io\]/) {
    in_patch = 1
    print
    next
  }

  if (in_patch && $0 ~ /^\[/ && !added) {
    print patch_line
    added = 1
    in_patch = 0
  }

  print
}
END {
  if (in_patch && !added) {
    print patch_line
    added = 1
  }
  if (!added) {
    print ""
    print "[patch.crates-io]"
    print patch_line
  }
}
' "$CARGO_TOML" > "$TMP_FILE"

mv "$TMP_FILE" "$CARGO_TOML"
echo "[rusty-kaspa-patch] added crates.io patch for workflow-perf-monitor to $CARGO_TOML"
