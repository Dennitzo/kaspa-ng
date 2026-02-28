#!/usr/bin/env bash

set -Eeuo pipefail
IFS=$'\n\t'

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DEBUG="${DEBUG:-1}"
SKIP_KASIA="${SKIP_KASIA:-0}"
SKIP_CARGO="${SKIP_CARGO:-0}"
SKIP_PACKAGE="${SKIP_PACKAGE:-0}"
BUILD_APPIMAGE="${BUILD_APPIMAGE:-auto}"
LOG_DIR="${LOG_DIR:-$ROOT_DIR/ci-local-logs}"
ARTIFACT_ROOT="${ARTIFACT_ROOT:-}"

usage() {
  cat <<'EOF'
Local GitHub-Artifact Build Simulation

Usage:
  scripts/local-artifact-sim.sh [options]

Options:
  --log-dir <path>      Log directory (default: ./ci-local-logs)
  --artifact-root <p>   Override artifact output directory name
  --skip-kasia          Skip Kasia npm/wasm build
  --skip-cargo          Skip cargo build --release
  --skip-package        Skip packaging and verification
  --build-appimage      Build AppImage too (Linux only)
  --no-appimage         Do not build AppImage
  --debug               Enable shell trace (default)
  --no-debug            Disable shell trace
  -h, --help            Show this help

Environment equivalents:
  LOG_DIR, ARTIFACT_ROOT, SKIP_KASIA=1, SKIP_CARGO=1, SKIP_PACKAGE=1,
  BUILD_APPIMAGE=auto|0|1, DEBUG=0|1
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --log-dir)
      LOG_DIR="$2"
      shift 2
      ;;
    --artifact-root)
      ARTIFACT_ROOT="$2"
      shift 2
      ;;
    --skip-kasia)
      SKIP_KASIA=1
      shift
      ;;
    --skip-cargo)
      SKIP_CARGO=1
      shift
      ;;
    --skip-package)
      SKIP_PACKAGE=1
      shift
      ;;
    --debug)
      DEBUG=1
      shift
      ;;
    --build-appimage)
      BUILD_APPIMAGE=1
      shift
      ;;
    --no-appimage)
      BUILD_APPIMAGE=0
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

mkdir -p "$LOG_DIR"

if [[ "$DEBUG" == "1" ]]; then
  export PS4='+ [${BASH_SOURCE##*/}:${LINENO}] '
  set -x
fi

trap 'echo "ERROR: command failed at line $LINENO" >&2' ERR

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Required command not found: $1" >&2
    exit 1
  }
}

require_cmd git
require_cmd cargo
require_cmd npm
require_cmd python3
require_cmd curl

is_compatible_kaspa_wasm_dir() {
  local dir="$1"
  local pkg="$dir/package.json"
  local js="$dir/kaspa.js"
  local dts="$dir/kaspa.d.ts"
  [[ -f "$pkg" && -f "$js" && -f "$dts" ]] || return 1
  grep -Eq '"name"[[:space:]]*:[[:space:]]*"kaspa-wasm"' "$pkg" || return 1
  grep -Eq 'export[[:space:]]+default' "$js" || return 1
  grep -Eq 'export[[:space:]]+class[[:space:]]+RpcClient' "$js" || return 1
  grep -Eq 'export[[:space:]]+const[[:space:]]+ConnectStrategy' "$js" || return 1
  grep -Eq 'export[[:space:]]+const[[:space:]]+Encoding' "$js" || return 1
  grep -Eq 'export[[:space:]]+class[[:space:]]+Resolver' "$js" || return 1
  grep -Eq 'export[[:space:]]+function[[:space:]]+initConsolePanicHook' "$js" || return 1
  return 0
}

find_kaspa_wasm_dir() {
  local search_root="$1"
  [[ -d "$search_root" ]] || return 1

  local direct_candidates=(
    "$search_root/kaspa-wasm32-sdk/web/kaspa"
    "$search_root/web/kaspa"
  )
  local candidate
  for candidate in "${direct_candidates[@]}"; do
    if is_compatible_kaspa_wasm_dir "$candidate"; then
      echo "$candidate"
      return 0
    fi
  done

  local pkg parent
  while IFS= read -r pkg; do
    parent="$(dirname "$pkg")"
    if is_compatible_kaspa_wasm_dir "$parent"; then
      echo "$parent"
      return 0
    fi
  done < <(find "$search_root" -type f -path '*/web/kaspa/package.json')

  return 1
}

prepare_kasia_wasm() {
  [[ -d Kasia ]] || return 0

  if is_compatible_kaspa_wasm_dir "Kasia/wasm"; then
    return 0
  fi

  rm -rf "Kasia/wasm"

  local tmp_root
  tmp_root="$(mktemp -d 2>/dev/null || mktemp -d -t kasiawasm)"
  trap 'rm -rf "$tmp_root"' RETURN

  local pkg_dir
  if pkg_dir="$(find_kaspa_wasm_dir "rusty-kaspa" 2>/dev/null)"; then
    mkdir -p "Kasia/wasm"
    cp -R "$pkg_dir"/. "Kasia/wasm/"
    is_compatible_kaspa_wasm_dir "Kasia/wasm"
    return 0
  fi

  local urls=(
    "https://github.com/IzioDev/rusty-kaspa/releases/download/v1.0.1-beta1/kaspa-wasm32-sdk-v1.0.1-beta1.zip"
    "https://github.com/kaspanet/rusty-kaspa/releases/download/v1.0.0/kaspa-wasm32-sdk-v1.0.0.zip"
  )
  local url zip_path extract_dir
  for url in "${urls[@]}"; do
    zip_path="${tmp_root}/sdk.zip"
    extract_dir="${tmp_root}/extract"
    rm -rf "$zip_path" "$extract_dir"
    if ! curl -fL --retry 2 -o "$zip_path" "$url"; then
      continue
    fi
    python3 -c 'import pathlib,sys,zipfile; p=pathlib.Path(sys.argv[1]); d=pathlib.Path(sys.argv[2]); d.mkdir(parents=True, exist_ok=True); zipfile.ZipFile(p).extractall(d)' "$zip_path" "$extract_dir"
    if pkg_dir="$(find_kaspa_wasm_dir "$extract_dir" 2>/dev/null)"; then
      mkdir -p "Kasia/wasm"
      cp -R "$pkg_dir"/. "Kasia/wasm/"
      is_compatible_kaspa_wasm_dir "Kasia/wasm"
      return 0
    fi
  done

  echo "Unable to locate compatible kaspa-wasm package for Kasia/wasm" >&2
  exit 1
}

build_kasia() {
  [[ -d Kasia ]] || return 0
  (
    cd Kasia
    if [[ -f package-lock.json ]]; then
      npm ci --prefer-offline --no-audit --no-fund || npm install --no-audit --no-fund
    else
      npm install --no-audit --no-fund
    fi
    npm run wasm:build || true
    npm run build:production || npm exec vite build
  )
}

build_explorer_if_missing() {
  [[ -d kaspa-explorer-ng ]] || return 0
  if [[ -d target/release/kaspa-explorer-ng || -d kaspa-explorer-ng/build ]]; then
    return 0
  fi

  echo "kaspa-explorer-ng build missing after cargo build; running fallback build"
  (
    cd kaspa-explorer-ng
    if [[ -f package-lock.json ]]; then
      npm ci --prefer-offline --no-audit --no-fund || npm install --no-audit --no-fund
    else
      npm install --no-audit --no-fund
    fi
    npm run build
  )
}

build_release() {
  KASIA_WASM_AUTO_FETCH=0 cargo build --release
}

detect_platform_suffix() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Darwin)
      if [[ "$arch" == "arm64" ]]; then
        echo "macos-arm64"
      else
        echo "macos-x64"
      fi
      ;;
    Linux)
      echo "linux-gnu-amd64"
      ;;
    *)
      echo "unknown"
      ;;
  esac
}

copy_binary_if_exists() {
  local bin="$1"
  local dest="$2"
  if [[ -f "target/release/${bin}" ]]; then
    cp "target/release/${bin}" "$dest/"
  elif [[ -f "rusty-kaspa/target/release/${bin}" ]]; then
    cp "rusty-kaspa/target/release/${bin}" "$dest/"
  elif [[ -f "cpuminer/target/release/${bin}" ]]; then
    cp "cpuminer/target/release/${bin}" "$dest/"
  elif [[ -f "simply-kaspa-indexer/target/release/${bin}" ]]; then
    cp "simply-kaspa-indexer/target/release/${bin}" "$dest/"
  elif [[ -f "K-indexer/target/release/${bin}" ]]; then
    cp "K-indexer/target/release/${bin}" "$dest/"
  elif [[ "$bin" == "kasia-indexer" && -f "kasia-indexer/target/release/indexer" ]]; then
    cp "kasia-indexer/target/release/indexer" "$dest/${bin}"
  fi
}

package_and_verify() {
  local short_sha platform root os
  short_sha="$(git rev-parse --short HEAD 2>/dev/null || echo "local")"
  platform="$(detect_platform_suffix)"
  root="${ARTIFACT_ROOT:-kaspa-ng-${short_sha}-${platform}-local-sim}"
  os="$(uname -s)"

  rm -rf "$root"
  mkdir -p "$root"

  build_explorer_if_missing

  if [[ "$os" == "Darwin" ]]; then
    "$ROOT_DIR/scripts/macos-bundle.sh" release
    [[ -d "$ROOT_DIR/target/release/Kaspa-NG.app" ]] || {
      echo "Missing macOS app bundle: target/release/Kaspa-NG.app" >&2
      exit 1
    }
    cp -R "$ROOT_DIR/target/release/Kaspa-NG.app" "$root/"
  fi

  cp target/release/kaspa-ng "$root/"

  local bin
  for bin in stratum-bridge kaspa-miner rothschild simply-kaspa-indexer K-webserver K-transaction-processor kasia-indexer; do
    copy_binary_if_exists "$bin" "$root"
  done

  if [[ -d target/release/kaspa-explorer-ng ]]; then
    cp -r target/release/kaspa-explorer-ng "$root/"
  elif [[ -d kaspa-explorer-ng/build ]]; then
    mkdir -p "$root/kaspa-explorer-ng"
    cp -r kaspa-explorer-ng/build "$root/kaspa-explorer-ng/"
  fi

  [[ -d kaspa-rest-server ]] && cp -r kaspa-rest-server "$root/"
  [[ -d kaspa-socket-server ]] && cp -r kaspa-socket-server "$root/"

  if [[ -d target/release/K/dist ]]; then
    mkdir -p "$root/K"
    cp -r target/release/K/dist "$root/K/"
  elif [[ -d K/dist ]]; then
    mkdir -p "$root/K"
    cp -r K/dist "$root/K/"
  fi

  if [[ -d target/release/Kasia/dist ]]; then
    mkdir -p "$root/Kasia"
    cp -r target/release/Kasia/dist "$root/Kasia/"
  elif [[ -d Kasia/dist ]]; then
    mkdir -p "$root/Kasia"
    cp -r Kasia/dist "$root/Kasia/"
  fi

  if [[ -d target/release/KasVault/build ]]; then
    mkdir -p "$root/KasVault"
    cp -r target/release/KasVault/build "$root/KasVault/"
  elif [[ -d kasvault/build ]]; then
    mkdir -p "$root/KasVault"
    cp -r kasvault/build "$root/KasVault/"
  fi

  for bin in kaspa-ng stratum-bridge kaspa-miner rothschild simply-kaspa-indexer K-webserver K-transaction-processor kasia-indexer; do
    [[ -f "$root/$bin" ]] || { echo "Missing packaged binary: $bin" >&2; exit 1; }
  done
  for dir in kaspa-explorer-ng kaspa-rest-server kaspa-socket-server K Kasia KasVault; do
    [[ -d "$root/$dir" ]] || { echo "Missing packaged directory: $dir" >&2; exit 1; }
  done
  if [[ "$os" == "Darwin" ]]; then
    [[ -d "$root/Kaspa-NG.app" ]] || { echo "Missing packaged app bundle: Kaspa-NG.app" >&2; exit 1; }
    [[ -f "$root/Kaspa-NG.app/Contents/Info.plist" ]] || { echo "Missing app Info.plist" >&2; exit 1; }
    [[ -f "$root/Kaspa-NG.app/Contents/MacOS/kaspa-ng" ]] || { echo "Missing app executable" >&2; exit 1; }
  fi

  echo "LOCAL_ARTIFACT_SIM_OK root=$root"

  local appimage_should_build
  appimage_should_build=0
  if [[ "$BUILD_APPIMAGE" == "1" ]]; then
    appimage_should_build=1
  elif [[ "$BUILD_APPIMAGE" == "auto" && "$os" == "Linux" ]]; then
    appimage_should_build=1
  fi

  if [[ "$appimage_should_build" == "1" ]]; then
    local appimage_out
    appimage_out="${root}.AppImage"
    if [[ -x "$ROOT_DIR/scripts/build-linux-appimage.sh" ]]; then
      "$ROOT_DIR/scripts/build-linux-appimage.sh" --input "$root" --output "$appimage_out"
      echo "LOCAL_APPIMAGE_OK file=$appimage_out"
    else
      echo "AppImage script missing or not executable: scripts/build-linux-appimage.sh" >&2
      exit 1
    fi
  fi
}

echo "==> [1/4] Prepare Kasia wasm package"
prepare_kasia_wasm 2>&1 | tee "$LOG_DIR/prepare-kasia-wasm.log"

if [[ "$SKIP_KASIA" != "1" ]]; then
  echo "==> [2/4] Build Kasia frontend"
  build_kasia 2>&1 | tee "$LOG_DIR/kasia-build.log"
else
  echo "==> [2/4] Skipped Kasia build"
fi

if [[ "$SKIP_CARGO" != "1" ]]; then
  echo "==> [3/4] Cargo release build"
  build_release 2>&1 | tee "$LOG_DIR/cargo-build-release.log"
else
  echo "==> [3/4] Skipped cargo build"
fi

if [[ "$SKIP_PACKAGE" != "1" ]]; then
  echo "==> [4/4] Package + verify artifact layout"
  package_and_verify 2>&1 | tee "$LOG_DIR/package-verify.log"
else
  echo "==> [4/4] Skipped package/verify"
fi

echo "Done. Logs in: $LOG_DIR"
