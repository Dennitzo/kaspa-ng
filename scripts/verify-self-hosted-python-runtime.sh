#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="${1:?Usage: $0 <artifact-root>}"

require_dir() {
  local path="$1"
  [ -d "$path" ] || {
    echo "Missing required directory: $path" >&2
    exit 1
  }
}

require_file() {
  local path="$1"
  [ -f "$path" ] || {
    echo "Missing required file: $path" >&2
    exit 1
  }
}

choose_python() {
  local -a raw_candidates=()
  local -a resolved_candidates=()
  local candidate resolved

  if [[ -n "${KASPA_NG_PYTHON_BIN:-}" ]]; then
    raw_candidates+=("${KASPA_NG_PYTHON_BIN}")
  fi
  raw_candidates+=("python3" "python")

  case "$(uname -s)" in
    Linux)
      raw_candidates+=("/usr/local/bin/python3" "/usr/bin/python3")
      ;;
    Darwin)
      raw_candidates+=(
        "/opt/homebrew/bin/python3"
        "/usr/local/bin/python3"
        "/usr/bin/python3"
      )
      ;;
  esac

  for candidate in "${raw_candidates[@]}"; do
    if [[ "$candidate" == */* ]]; then
      [[ -x "$candidate" ]] || continue
      resolved="$candidate"
    else
      resolved="$(command -v "$candidate" 2>/dev/null || true)"
      [[ -n "$resolved" ]] || continue
    fi
    resolved_candidates+=("$resolved")
  done

  for resolved in "${resolved_candidates[@]}"; do
    if "$resolved" - <<'PY' >/dev/null 2>&1
import sys
raise SystemExit(0 if (sys.version_info.major == 3 and sys.version_info.minor >= 10) else 1)
PY
    then
      echo "$resolved"
      return 0
    fi
  done

  return 1
}

check_server_venv_modules() {
  local server_name="$1"
  local server_root="$2"
  shift 2
  local venv_python=""
  local candidate

  for candidate in \
    "$server_root/.venv/bin/python3" \
    "$server_root/.venv/bin/python" \
    "$server_root/.venv/Scripts/python.exe"; do
    if [[ -x "$candidate" ]]; then
      venv_python="$candidate"
      break
    fi
  done

  if [[ -z "$venv_python" ]]; then
    echo "[python-runtime] ${server_name}: no packaged .venv found (runtime bootstrap expected)"
    return 0
  fi

  if ! "$venv_python" - "$@" <<'PY' >/dev/null 2>&1
import importlib.util
import sys
missing = [m for m in sys.argv[1:] if importlib.util.find_spec(m) is None]
raise SystemExit(0 if not missing else 1)
PY
  then
    echo "[python-runtime] ${server_name}: packaged venv is missing required modules" >&2
    exit 1
  fi
}

REST_ROOT="$ROOT/kaspa-rest-server"
SOCKET_ROOT="$ROOT/kaspa-socket-server"

require_dir "$REST_ROOT"
require_dir "$SOCKET_ROOT"
require_file "$REST_ROOT/main.py"
require_file "$SOCKET_ROOT/main.py"
require_file "$REST_ROOT/pyproject.toml"
require_file "$SOCKET_ROOT/Pipfile"

PYTHON_BIN="$(choose_python || true)"
if [[ -z "$PYTHON_BIN" ]]; then
  echo "No compatible Python runtime (>=3.10) found for self-hosted services" >&2
  exit 1
fi

if ! "$PYTHON_BIN" - <<'PY' >/dev/null 2>&1
import importlib.util
missing = [m for m in ("venv", "ensurepip") if importlib.util.find_spec(m) is None]
raise SystemExit(0 if not missing else 1)
PY
then
  echo "Python runtime is missing venv/ensurepip support: $PYTHON_BIN" >&2
  exit 1
fi

if ! "$PYTHON_BIN" -m venv --help >/dev/null 2>&1; then
  echo "Python runtime cannot execute venv module: $PYTHON_BIN" >&2
  exit 1
fi

if ! "$PYTHON_BIN" -m pip --version >/dev/null 2>&1; then
  if ! "$PYTHON_BIN" -m ensurepip --help >/dev/null 2>&1; then
    echo "Python runtime has neither pip nor ensurepip available: $PYTHON_BIN" >&2
    exit 1
  fi
fi

check_server_venv_modules \
  "kaspa-rest-server" \
  "$REST_ROOT" \
  fastapi \
  uvicorn \
  pydantic \
  pydantic_settings \
  asyncpg \
  psycopg2

check_server_venv_modules \
  "kaspa-socket-server" \
  "$SOCKET_ROOT" \
  fastapi \
  uvicorn \
  pydantic \
  pydantic_settings \
  socketio \
  engineio

echo "Self-hosted Python runtime verification passed: $ROOT (python: $PYTHON_BIN)"
