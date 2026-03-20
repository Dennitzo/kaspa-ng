#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# LLVM Workaround Installer for macOS Monterey on Intel Mac Pro 6,1
#
# Strategie:
#   1) Versuche offizielles Prebuilt-Tarball zu installieren
#   2) Falls nicht verfügbar / gewünscht: Build from source
#
# Default-Version:
#   15.0.7
# Grund:
#   Für Intel/macOS existierte hierfür offiziell:
#   clang+llvm-15.0.7-x86_64-apple-darwin21.0.tar.xz
#
# Nutzung:
#   chmod +x install-llvm-monterey.sh
#   ./install-llvm-monterey.sh
#
# Optional:
#   LLVM_VERSION=15.0.7 ./install-llvm-monterey.sh
#   INSTALL_PREFIX="$HOME/.local/llvm-15.0.7" ./install-llvm-monterey.sh
#   FORCE_SOURCE_BUILD=1 ./install-llvm-monterey.sh
#   JOBS=8 ./install-llvm-monterey.sh
# ============================================================================

LLVM_VERSION="${LLVM_VERSION:-15.0.7}"
INSTALL_PREFIX="${INSTALL_PREFIX:-/usr/local/opt/llvm-${LLVM_VERSION}}"
WORKDIR="${WORKDIR:-$HOME/.cache/llvm-installer}"
JOBS="${JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || echo 8)}"
FORCE_SOURCE_BUILD="${FORCE_SOURCE_BUILD:-0}"

ARCH="$(uname -m)"
OS_NAME="$(uname -s)"
MACOS_VERSION="$(sw_vers -productVersion)"
DARWIN_MAJOR="$(uname -r | cut -d. -f1)"

# Monterey = macOS 12.x = Darwin 21.x
EXPECTED_DARWIN="21"

log() {
  printf '\033[1;34m[INFO]\033[0m %s\n' "$*"
}

warn() {
  printf '\033[1;33m[WARN]\033[0m %s\n' "$*" >&2
}

err() {
  printf '\033[1;31m[ERR ]\033[0m %s\n' "$*" >&2
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    err "Benötigtes Kommando fehlt: $1"
    exit 1
  }
}

check_platform() {
  if [[ "$OS_NAME" != "Darwin" ]]; then
    err "Dieses Skript ist nur für macOS."
    exit 1
  fi

  if [[ "$ARCH" != "x86_64" ]]; then
    err "Dieses Skript ist auf Intel/x86_64 ausgelegt. Erkannt: $ARCH"
    exit 1
  fi

  if [[ "$DARWIN_MAJOR" != "$EXPECTED_DARWIN" ]]; then
    warn "Erkanntes Darwin: $DARWIN_MAJOR"
    warn "Dieses Skript ist primär für macOS Monterey (Darwin 21) gedacht."
    warn "Ich fahre trotzdem fort."
  fi

  log "Plattform: macOS $MACOS_VERSION, Darwin $DARWIN_MAJOR, Arch $ARCH"
}

check_xcode_clt() {
  if ! xcode-select -p >/dev/null 2>&1; then
    err "Xcode Command Line Tools fehlen."
    err "Bitte zuerst ausführen:"
    err "  xcode-select --install"
    exit 1
  fi
}

prepare_dirs() {
  mkdir -p "$WORKDIR"
  mkdir -p "$(dirname "$INSTALL_PREFIX")"
}

download_file() {
  local url="$1"
  local out="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -L --fail --retry 3 --retry-delay 2 -o "$out" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "$out" "$url"
  else
    err "Weder curl noch wget gefunden."
    exit 1
  fi
}

install_prebuilt() {
  local triple="x86_64-apple-darwin21.0"
  local file="clang+llvm-${LLVM_VERSION}-${triple}.tar.xz"
  local url="https://github.com/llvm/llvm-project/releases/download/llvmorg-${LLVM_VERSION}/${file}"
  local archive="${WORKDIR}/${file}"
  local extract_dir="${WORKDIR}/extract-${LLVM_VERSION}"

  log "Versuche offizielles Prebuilt-Binary:"
  log "  $url"

  rm -rf "$extract_dir"
  mkdir -p "$extract_dir"

  if ! download_file "$url" "$archive"; then
    warn "Prebuilt-Binary nicht verfügbar oder Download fehlgeschlagen."
    return 1
  fi

  need_cmd tar

  log "Entpacke Archiv ..."
  tar -xJf "$archive" -C "$extract_dir"

  local unpacked_dir
  unpacked_dir="$(find "$extract_dir" -maxdepth 1 -type d -name "clang+llvm-*" | head -n 1 || true)"

  if [[ -z "$unpacked_dir" ]]; then
    warn "Konnte entpacktes LLVM-Verzeichnis nicht finden."
    return 1
  fi

  log "Installiere nach: $INSTALL_PREFIX"
  rm -rf "$INSTALL_PREFIX"
  mkdir -p "$INSTALL_PREFIX"
  cp -R "$unpacked_dir"/. "$INSTALL_PREFIX"/

  return 0
}

build_from_source() {
  local src_archive="llvm-project-${LLVM_VERSION}.src.tar.xz"
  local src_url="https://github.com/llvm/llvm-project/releases/download/llvmorg-${LLVM_VERSION}/${src_archive}"
  local src_path="${WORKDIR}/${src_archive}"
  local src_root="${WORKDIR}/src-${LLVM_VERSION}"
  local build_root="${WORKDIR}/build-${LLVM_VERSION}"

  log "Starte Build from source ..."
  log "Quelle: $src_url"

  if ! command -v cmake >/dev/null 2>&1; then
    err "cmake fehlt."
    err "Für den Source-Build brauchst du cmake."
    err "Optionen:"
    err "  - cmake manuell installieren"
    err "  - oder zuerst nur das Prebuilt verwenden"
    exit 1
  fi

  if command -v ninja >/dev/null 2>&1; then
    local generator="Ninja"
    local build_cmd=(cmake --build "$build_root" --parallel "$JOBS")
  else
    warn "ninja nicht gefunden, verwende Unix Makefiles."
    local generator="Unix Makefiles"
    local build_cmd=(cmake --build "$build_root" --parallel "$JOBS")
  fi

  rm -rf "$src_root" "$build_root"
  mkdir -p "$src_root" "$build_root"

  download_file "$src_url" "$src_path"
  tar -xJf "$src_path" -C "$src_root"

  local unpacked_dir
  unpacked_dir="$(find "$src_root" -maxdepth 1 -type d -name "llvm-project-*" | head -n 1 || true)"

  if [[ -z "$unpacked_dir" ]]; then
    err "Konnte Source-Verzeichnis nicht finden."
    exit 1
  fi

  log "Konfiguriere Build ..."
  cmake -S "${unpacked_dir}/llvm" -B "$build_root" \
    -G "$generator" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$INSTALL_PREFIX" \
    -DLLVM_ENABLE_PROJECTS="clang;lld;clang-tools-extra" \
    -DLLVM_TARGETS_TO_BUILD="X86;AArch64" \
    -DLLVM_ENABLE_ASSERTIONS=OFF \
    -DLLVM_INCLUDE_TESTS=OFF \
    -DLLVM_INCLUDE_BENCHMARKS=OFF \
    -DLLVM_INCLUDE_EXAMPLES=OFF \
    -DCLANG_INCLUDE_TESTS=OFF

  log "Baue LLVM/Clang ..."
  "${build_cmd[@]}"

  log "Installiere ..."
  cmake --install "$build_root"
}

write_env_file() {
  local env_file="${INSTALL_PREFIX}/enable-llvm-env.sh"

  cat > "$env_file" <<EOF
# LLVM environment for version ${LLVM_VERSION}
export LLVM_HOME="${INSTALL_PREFIX}"
export PATH="\$LLVM_HOME/bin:\$PATH"
export CC="\$LLVM_HOME/bin/clang"
export CXX="\$LLVM_HOME/bin/clang++"
export LDFLAGS="-L\$LLVM_HOME/lib \$LDFLAGS"
export CPPFLAGS="-I\$LLVM_HOME/include \$CPPFLAGS"
export PKG_CONFIG_PATH="\$LLVM_HOME/lib/pkgconfig:\${PKG_CONFIG_PATH:-}"
EOF

  log "Env-Datei geschrieben: $env_file"
  log "Aktivieren mit:"
  log "  source \"$env_file\""
}

configure_zshrc() {
  local shell_rc="${SHELL_RC_FILE:-$HOME/.zshrc}"
  local block_start="# >>> llvm-installer managed block >>>"
  local block_end="# <<< llvm-installer managed block <<<"
  local fn_suffix
  fn_suffix="$(printf '%s' "$LLVM_VERSION" | tr '.' '_')"
  local fn_name="use_llvm_${fn_suffix}"
  local tmp_file
  tmp_file="${shell_rc}.tmp.$$"

  touch "$shell_rc"

  # Remove an older managed block to keep the update idempotent.
  if grep -Fq "$block_start" "$shell_rc"; then
    awk -v start="$block_start" -v end="$block_end" '
      $0 == start { skip = 1; next }
      $0 == end { skip = 0; next }
      !skip { print }
    ' "$shell_rc" > "$tmp_file"
    mv "$tmp_file" "$shell_rc"
  fi

  cat >> "$shell_rc" <<EOF

$block_start
export LLVM_HOME="$INSTALL_PREFIX"
if [[ ":\$PATH:" != *":\$LLVM_HOME/bin:"* ]]; then
  export PATH="\$LLVM_HOME/bin:\$PATH"
fi
if [[ -d "/usr/local/opt/node-22/bin" ]] && [[ ":\$PATH:" != *":/usr/local/opt/node-22/bin:"* ]]; then
  export PATH="/usr/local/opt/node-22/bin:\$PATH"
fi
if [[ -x "/usr/local/opt/node-22/bin/npm" ]]; then
  export NPM="/usr/local/opt/node-22/bin/npm"
fi
export LDFLAGS="-L\$LLVM_HOME/lib"
export CPPFLAGS="-I\$LLVM_HOME/include"

# Keep cargo installs compatible by default; enable LLVM compiler tools explicitly.
unset CC CXX AR RANLIB

$fn_name() {
  export CC="\$LLVM_HOME/bin/clang"
  export CXX="\$LLVM_HOME/bin/clang++"
  export AR="\$LLVM_HOME/bin/llvm-ar"
  export RANLIB="\$LLVM_HOME/bin/llvm-ranlib"
  echo "LLVM toolchain enabled for this shell session: \$LLVM_HOME"
}
$block_end
EOF

  log "~/.zshrc aktualisiert: $shell_rc"
  log "Für LLVM-Compiler-Tools bei Bedarf in neuer Shell aufrufen:"
  log "  $fn_name"
}

test_install() {
  log "Prüfe Installation ..."
  if [[ ! -x "${INSTALL_PREFIX}/bin/clang" ]]; then
    err "clang wurde nicht installiert."
    exit 1
  fi

  "${INSTALL_PREFIX}/bin/clang" --version || true
  "${INSTALL_PREFIX}/bin/llvm-config" --version || true

  log "Fertig."
  log "LLVM liegt unter:"
  log "  $INSTALL_PREFIX"
  log
  log "Temporär aktivieren:"
  log "  source \"$INSTALL_PREFIX/enable-llvm-env.sh\""
  log
  log "Dauerhafte zsh-Konfiguration wurde automatisch gesetzt."
  log "Neue Shell laden mit:"
  log "  exec zsh -l"
}

main() {
  check_platform
  check_xcode_clt
  prepare_dirs

  if [[ "$FORCE_SOURCE_BUILD" == "1" ]]; then
    build_from_source
  else
    if ! install_prebuilt; then
      warn "Falle auf Source-Build zurück."
      build_from_source
    fi
  fi

  write_env_file
  configure_zshrc
  test_install
}

main "$@"
