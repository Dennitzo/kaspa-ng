#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-${ROOT}/target/release/postgres}"
SRC="${ROOT}/postgres"
REQUIRED_BINS=(postgres initdb pg_ctl)
EXPECTED_MAJOR="15"

mkdir -p "$OUT"

copy_tree() {
  local src="$1"
  local dst="$2"
  [ -d "$src" ] || return 1
  mkdir -p "$dst"
  (cd "$src" && tar -cf - .) | (cd "$dst" && tar -xf -)
  return 0
}

postgres_major() {
  local exe="$1"
  local version_line major
  version_line="$("$exe" --version 2>/dev/null || true)"
  major="$(printf '%s' "$version_line" | grep -Eo '[0-9]+' | head -n1 || true)"
  printf '%s' "$major"
}

has_required_bins() {
  local root="$1"
  local bin
  for bin in "${REQUIRED_BINS[@]}"; do
    [ -x "$root/bin/$bin" ] || return 1
  done
  return 0
}

is_expected_runtime_root() {
  local root="$1"
  has_required_bins "$root" || return 1
  local major
  major="$(postgres_major "$root/bin/postgres")"
  [ "$major" = "$EXPECTED_MAJOR" ] || return 1
  "$root/bin/initdb" --version >/dev/null 2>&1 || return 1
  "$root/bin/pg_ctl" --version >/dev/null 2>&1 || return 1
  return 0
}

validate_staged_output() {
  local root="$1"
  is_expected_runtime_root "$root"
}

if ! is_expected_runtime_root "$SRC"; then
  fetch_script="${ROOT}/scripts/fetch-postgres-runtime.sh"
  if [ -f "$fetch_script" ]; then
    echo "[postgres] bundled runtime missing or unsupported at $SRC; attempting automatic fetch"
    bash "$fetch_script" "$EXPECTED_MAJOR"
  fi
fi

if ! is_expected_runtime_root "$SRC"; then
  echo "Bundled PostgreSQL runtime missing or unsupported at $SRC (expected major ${EXPECTED_MAJOR})." >&2
  echo "Required binaries: ${SRC}/bin/postgres, ${SRC}/bin/initdb, ${SRC}/bin/pg_ctl" >&2
  exit 1
fi

if [ "$OUT" = "$SRC" ]; then
  echo "PostgreSQL runtime already staged at $OUT (major ${EXPECTED_MAJOR})"
  exit 0
fi

stage_tree() {
  rm -rf "$OUT/bin" "$OUT/lib" "$OUT/share"
  copy_tree "$SRC/bin" "$OUT/bin" || return 1
  copy_tree "$SRC/lib" "$OUT/lib" || return 1
  copy_tree "$SRC/share" "$OUT/share" || return 1
  validate_staged_output "$OUT" || return 1
  echo "Staged PostgreSQL runtime from bundled source: ${SRC} -> $OUT"
  return 0
}

if stage_tree; then
  exit 0
fi

echo "Unable to stage bundled PostgreSQL runtime from $SRC to $OUT." >&2
exit 1
