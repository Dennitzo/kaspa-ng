#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-${ROOT}/target/release/python}"
SRC="${ROOT}/python"
MIN_MINOR="${KASPA_NG_PYTHON_MIN_MINOR:-10}"
MAX_MINOR="${KASPA_NG_PYTHON_MAX_MINOR:-13}"

copy_tree() {
  local src="$1"
  local dst="$2"
  [ -d "$src" ] || return 1
  mkdir -p "$dst"
  (cd "$src" && tar -cf - .) | (cd "$dst" && tar -xf -)
  return 0
}

python_version_minor() {
  local exe="$1"
  "$exe" - <<'PY' 2>/dev/null || true
import sys
print(f"{sys.version_info.major}.{sys.version_info.minor}")
PY
}

is_expected_runtime_root() {
  local root="$1"
  local py=""
  for candidate in "$root/bin/python3" "$root/bin/python"; do
    if [ -x "$candidate" ]; then
      py="$candidate"
      break
    fi
  done
  [ -n "$py" ] || return 1

  local version major minor
  version="$(python_version_minor "$py")"
  major="${version%%.*}"
  minor="${version#*.}"
  [ "$major" = "3" ] || return 1
  [[ "$minor" =~ ^[0-9]+$ ]] || return 1
  [ "$minor" -ge "$MIN_MINOR" ] || return 1
  [ "$minor" -le "$MAX_MINOR" ] || return 1

  "$py" - <<'PY' >/dev/null 2>&1 || return 1
import importlib.util
missing = [m for m in ("venv", "ensurepip") if importlib.util.find_spec(m) is None]
raise SystemExit(0 if not missing else 1)
PY
  "$py" -m venv --help >/dev/null 2>&1 || return 1
  "$py" -m ensurepip --help >/dev/null 2>&1 || return 1
  return 0
}

if ! is_expected_runtime_root "$SRC"; then
  fetch_script="${ROOT}/scripts/fetch-python-runtime.sh"
  if [ -f "$fetch_script" ]; then
    echo "[python] bundled runtime missing or unsupported at $SRC; attempting automatic fetch"
    bash "$fetch_script"
  fi
fi

if ! is_expected_runtime_root "$SRC"; then
  echo "Bundled Python runtime missing or unsupported at $SRC." >&2
  echo "Expected executable: $SRC/bin/python3 (or $SRC/bin/python)." >&2
  exit 1
fi

if [ "$OUT" = "$SRC" ]; then
  echo "Python runtime already staged at $OUT"
  exit 0
fi

rm -rf "$OUT"
mkdir -p "$OUT"
copy_tree "$SRC" "$OUT"

if ! is_expected_runtime_root "$OUT"; then
  echo "Unable to stage bundled Python runtime from $SRC to $OUT." >&2
  exit 1
fi

echo "[python] staged runtime from '$SRC' to '$OUT'"
