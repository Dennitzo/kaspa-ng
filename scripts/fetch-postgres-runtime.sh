#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="${ROOT}/postgres"
EXPECTED_MAJOR="${1:-${KASPA_NG_POSTGRES_EXPECTED_MAJOR:-15}}"
FORCE_FETCH="${KASPA_NG_POSTGRES_RUNTIME_FORCE:-0}"

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

is_expected_runtime_root() {
  local root="$1"
  [ -x "$root/bin/postgres" ] || return 1
  [ -x "$root/bin/initdb" ] || return 1
  [ -x "$root/bin/pg_ctl" ] || return 1
  local major
  major="$(postgres_major "$root/bin/postgres")"
  [ "$major" = "$EXPECTED_MAJOR" ] || return 1
  "$root/bin/initdb" --version >/dev/null 2>&1 || return 1
  "$root/bin/pg_ctl" --version >/dev/null 2>&1 || return 1
  return 0
}

run_with_sudo() {
  if [ "$(id -u)" -eq 0 ]; then
    "$@"
    return 0
  fi
  if command -v sudo >/dev/null 2>&1; then
    sudo "$@"
    return 0
  fi
  echo "Command requires root privileges and sudo is not available: $*" >&2
  return 1
}

prepare_from_linux() {
  local lib_root="${KASPA_NG_POSTGRES_LINUX_LIB_ROOT:-/usr/lib/postgresql/${EXPECTED_MAJOR}}"
  local share_root="${KASPA_NG_POSTGRES_LINUX_SHARE_ROOT:-/usr/share/postgresql/${EXPECTED_MAJOR}}"

  if [ ! -x "$lib_root/bin/postgres" ]; then
    if ! command -v apt-get >/dev/null 2>&1; then
      echo "Missing PostgreSQL ${EXPECTED_MAJOR} binaries and apt-get is not available." >&2
      return 1
    fi
    run_with_sudo apt-get update
    run_with_sudo apt-get install -y "postgresql-${EXPECTED_MAJOR}" "postgresql-client-${EXPECTED_MAJOR}"
  fi

  [ -x "$lib_root/bin/postgres" ] || {
    echo "PostgreSQL binary not found at $lib_root/bin/postgres after install." >&2
    return 1
  }
  [ -d "$lib_root/lib" ] || {
    echo "PostgreSQL lib directory missing at $lib_root/lib." >&2
    return 1
  }
  [ -d "$share_root" ] || {
    echo "PostgreSQL share directory missing at $share_root." >&2
    return 1
  }

  rm -rf "$SRC"
  mkdir -p "$SRC"
  copy_tree "$lib_root/bin" "$SRC/bin"
  copy_tree "$lib_root/lib" "$SRC/lib"
  copy_tree "$share_root" "$SRC/share"
}

prepare_from_macos() {
  if ! command -v brew >/dev/null 2>&1; then
    echo "Homebrew is required to install PostgreSQL ${EXPECTED_MAJOR} on macOS." >&2
    return 1
  fi

  local formula="postgresql@${EXPECTED_MAJOR}"
  if ! brew list --versions "$formula" >/dev/null 2>&1; then
    brew update
    brew install "$formula"
  fi

  local prefix="${KASPA_NG_POSTGRES_MACOS_PREFIX:-$(brew --prefix "$formula")}"
  local share_root=""
  if [ -d "$prefix/share" ]; then
    share_root="$prefix/share"
  elif [ -d "$(brew --prefix)/share/${formula}" ]; then
    share_root="$(brew --prefix)/share/${formula}"
  fi

  [ -x "$prefix/bin/postgres" ] || {
    echo "PostgreSQL binary not found at $prefix/bin/postgres." >&2
    return 1
  }
  [ -d "$prefix/lib" ] || {
    echo "PostgreSQL lib directory missing at $prefix/lib." >&2
    return 1
  }
  [ -n "$share_root" ] || {
    echo "Unable to locate PostgreSQL share directory for $formula." >&2
    return 1
  }

  rm -rf "$SRC"
  mkdir -p "$SRC"
  copy_tree "$prefix/bin" "$SRC/bin"
  copy_tree "$prefix/lib" "$SRC/lib"
  copy_tree "$share_root" "$SRC/share"
}

if [ "$FORCE_FETCH" != "1" ] && is_expected_runtime_root "$SRC"; then
  echo "[postgres] bundled runtime already available at '$SRC' (major ${EXPECTED_MAJOR})"
  exit 0
fi

case "$(uname -s)" in
  Linux)
    prepare_from_linux
    ;;
  Darwin)
    prepare_from_macos
    ;;
  *)
    echo "Unsupported platform for automatic PostgreSQL runtime fetch: $(uname -s)" >&2
    exit 1
    ;;
esac

if ! is_expected_runtime_root "$SRC"; then
  echo "Fetched PostgreSQL runtime at '$SRC' is invalid or not major ${EXPECTED_MAJOR}." >&2
  exit 1
fi

echo "[postgres] bundled runtime prepared at '$SRC' (major ${EXPECTED_MAJOR})"
