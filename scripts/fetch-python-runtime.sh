#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="${ROOT}/python"
MIN_MINOR="${KASPA_NG_PYTHON_MIN_MINOR:-10}"
MAX_MINOR="${KASPA_NG_PYTHON_MAX_MINOR:-13}"
FORCE_FETCH="${KASPA_NG_PYTHON_RUNTIME_FORCE:-0}"

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

python_base_prefix() {
  local exe="$1"
  "$exe" - <<'PY' 2>/dev/null || true
import sys
print(sys.base_prefix)
PY
}

python_real_executable() {
  local exe="$1"
  "$exe" - <<'PY' 2>/dev/null || true
import os,sys
print(os.path.realpath(sys.executable))
PY
}

python_runtime_valid() {
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

prefer_python_candidates() {
  local -a candidates=()

  if [ -n "${KASPA_NG_PYTHON_BIN:-}" ]; then
    candidates+=("${KASPA_NG_PYTHON_BIN}")
  fi

  candidates+=("python3.13" "python3.12" "python3.11" "python3.10" "python3")

  printf '%s\n' "${candidates[@]}"
}

resolve_python_exe() {
  local candidate resolved
  while IFS= read -r candidate; do
    [ -n "$candidate" ] || continue
    if [[ "$candidate" == */* ]]; then
      [ -x "$candidate" ] || continue
      resolved="$candidate"
    else
      resolved="$(command -v "$candidate" 2>/dev/null || true)"
      [ -n "$resolved" ] || continue
    fi

    local version major minor
    version="$(python_version_minor "$resolved")"
    major="${version%%.*}"
    minor="${version#*.}"
    [ "$major" = "3" ] || continue
    [[ "$minor" =~ ^[0-9]+$ ]] || continue
    [ "$minor" -ge "$MIN_MINOR" ] || continue
    [ "$minor" -le "$MAX_MINOR" ] || continue
    echo "$resolved"
    return 0
  done < <(prefer_python_candidates)

  return 1
}

prepare_from_unix() {
  local py_exe py_root py_real py_prefix
  py_exe="$(resolve_python_exe || true)"
  [ -n "$py_exe" ] || {
    echo "Unable to find compatible Python runtime on host (need 3.${MIN_MINOR}-3.${MAX_MINOR})." >&2
    return 1
  }

  py_real="$(python_real_executable "$py_exe")"
  py_prefix="$(python_base_prefix "$py_exe")"
  [ -n "$py_real" ] || {
    echo "Unable to resolve real executable path for: $py_exe" >&2
    return 1
  }
  [ -n "$py_prefix" ] || {
    echo "Unable to resolve Python base prefix for: $py_exe" >&2
    return 1
  }

  py_root="$py_prefix"
  [ -d "$py_root/lib" ] || {
    echo "Resolved Python root has no lib directory: $py_root" >&2
    return 1
  }

  rm -rf "$SRC"
  mkdir -p "$SRC/bin"
  cp -L "$py_real" "$SRC/bin/python3"
  cp -L "$py_real" "$SRC/bin/python"
  copy_tree "$py_root/lib" "$SRC/lib" || true
  copy_tree "$py_root/include" "$SRC/include" || true
  copy_tree "$py_root/share" "$SRC/share" || true
}

if [ "$FORCE_FETCH" != "1" ] && python_runtime_valid "$SRC"; then
  echo "[python] bundled runtime already available at '$SRC'"
  exit 0
fi

case "$(uname -s)" in
  Linux|Darwin)
    prepare_from_unix
    ;;
  *)
    echo "Unsupported platform for automatic Python runtime fetch: $(uname -s)" >&2
    exit 1
    ;;
esac

if ! python_runtime_valid "$SRC"; then
  echo "Fetched Python runtime at '$SRC' is invalid or outside supported version range." >&2
  exit 1
fi

echo "[python] bundled runtime prepared at '$SRC'"
