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

copy_linux_runtime_deps() {
  local src_root="$1"
  local dst_lib="$2"
  local bins=(postgres initdb pg_ctl)
  local skip_re='^(/lib.*/ld-linux.*|/lib.*/libc\.so|/lib.*/libm\.so|/lib.*/libpthread\.so|/lib.*/librt\.so|/lib.*/libdl\.so|/lib.*/libresolv\.so)(\..*)?$'
  local bin dep target

  mkdir -p "$dst_lib"

  for bin in "${bins[@]}"; do
    if [ ! -x "$src_root/bin/$bin" ]; then
      continue
    fi
    while IFS= read -r dep; do
      [ -n "$dep" ] || continue
      if [[ "$dep" =~ $skip_re ]]; then
        continue
      fi
      target="$dst_lib/$(basename "$dep")"
      if [ ! -e "$target" ]; then
        cp -L "$dep" "$target"
      fi
    done < <(ldd "$src_root/bin/$bin" 2>/dev/null | awk '/=> \// {print $3}')
  done
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
    return $?
  fi
  if command -v sudo >/dev/null 2>&1; then
    sudo "$@"
    return $?
  fi
  echo "Command requires root privileges and sudo is not available: $*" >&2
  return 1
}

linux_codename() {
  local codename=""
  if [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    codename="${VERSION_CODENAME:-${UBUNTU_CODENAME:-}}"
  fi
  if [ -z "$codename" ] && command -v lsb_release >/dev/null 2>&1; then
    codename="$(lsb_release -cs 2>/dev/null || true)"
  fi
  printf '%s' "$codename"
}

install_linux_postgres_packages() {
  run_with_sudo apt-get install -y "postgresql-${EXPECTED_MAJOR}" "postgresql-client-${EXPECTED_MAJOR}"
}

setup_pgdg_repo() {
  local codename key_url key_tmp keyring_tmp
  codename="$(linux_codename)"
  if [ -z "$codename" ]; then
    echo "Unable to determine Linux codename for PostgreSQL apt repository setup." >&2
    return 1
  fi

  key_url="https://www.postgresql.org/media/keys/ACCC4CF8.asc"
  key_tmp="$(mktemp)"
  keyring_tmp="${key_tmp}.gpg"

  run_with_sudo apt-get install -y ca-certificates curl gnupg lsb-release
  curl -fsSL "$key_url" -o "$key_tmp"
  gpg --dearmor --yes --output "$keyring_tmp" "$key_tmp"

  run_with_sudo install -d -m 0755 /etc/apt/keyrings
  run_with_sudo install -m 0644 "$keyring_tmp" /etc/apt/keyrings/postgresql.gpg
  printf 'deb [signed-by=/etc/apt/keyrings/postgresql.gpg] https://apt.postgresql.org/pub/repos/apt %s-pgdg main\n' "$codename" \
    | run_with_sudo tee /etc/apt/sources.list.d/pgdg.list >/dev/null

  rm -f "$key_tmp" "$keyring_tmp"
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
    if ! install_linux_postgres_packages; then
      echo "[postgres] install from default apt repositories failed, attempting PGDG repository setup"
      setup_pgdg_repo
      run_with_sudo apt-get update
      install_linux_postgres_packages
    fi
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
  copy_linux_runtime_deps "$SRC" "$SRC/lib"
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
