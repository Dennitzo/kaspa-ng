#!/usr/bin/env bash

set -Eeuo pipefail
IFS=$'\n\t'

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOG_DIR="${LOG_DIR:-$ROOT_DIR/ci-local-logs}"
RUN_FMT=1
RUN_CLIPPY=1
RUN_CHECK=1
RUN_TEST=1
DEBUG="${DEBUG:-0}"

usage() {
  cat <<'USAGE'
Run local CI-like Rust checks (GitHub-style).

Usage:
  scripts/local-ci-checks.sh [options]

Options:
  --no-fmt         Skip cargo fmt check
  --no-clippy      Skip cargo clippy
  --no-check       Skip cargo check
  --no-test        Skip test step
  --log-dir <dir>  Log directory (default: ./ci-local-logs)
  --debug          Enable shell trace
  --no-debug       Disable shell trace
  -h, --help       Show this help

Environment:
  LOG_DIR, DEBUG=0|1
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-fmt)
      RUN_FMT=0
      shift
      ;;
    --no-clippy)
      RUN_CLIPPY=0
      shift
      ;;
    --no-check)
      RUN_CHECK=0
      shift
      ;;
    --no-test)
      RUN_TEST=0
      shift
      ;;
    --log-dir)
      LOG_DIR="$2"
      shift 2
      ;;
    --debug)
      DEBUG=1
      shift
      ;;
    --no-debug)
      DEBUG=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ "$DEBUG" == "1" ]]; then
  export PS4='+ [${BASH_SOURCE##*/}:${LINENO}] '
  set -x
fi

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Required command not found: $1" >&2
    exit 1
  }
}

run_step() {
  local name="$1"
  local logfile="$2"
  shift 2

  echo "==> $name"
  set -o pipefail
  "$@" 2>&1 | tee "$logfile"
}

mkdir -p "$LOG_DIR"
require_cmd cargo

if [[ "$RUN_FMT" == "1" ]]; then
  run_step "cargo fmt" "$LOG_DIR/cargo-fmt.log" \
    cargo fmt --all -- --check
fi

if [[ "$RUN_CLIPPY" == "1" ]]; then
  run_step "cargo clippy" "$LOG_DIR/cargo-clippy.log" \
    cargo clippy --workspace --all-targets --tests --benches -- -D warnings
fi

if [[ "$RUN_CHECK" == "1" ]]; then
  run_step "cargo check" "$LOG_DIR/cargo-check.log" \
    cargo check --tests --workspace --benches
fi

if [[ "$RUN_TEST" == "1" ]]; then
  if cargo nextest --version >/dev/null 2>&1; then
    run_step "cargo nextest run" "$LOG_DIR/cargo-nextest.log" \
      cargo nextest run --release --workspace
  else
    run_step "cargo test" "$LOG_DIR/cargo-test.log" \
      cargo test --release --workspace
  fi
fi

echo "All requested CI-like steps finished successfully."
echo "Logs: $LOG_DIR"
