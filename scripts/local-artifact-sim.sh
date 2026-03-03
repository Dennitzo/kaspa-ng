#!/usr/bin/env bash

set -Eeuo pipefail
IFS=$'\n\t'

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DEBUG="${DEBUG:-1}"
SKIP_KASIA="${SKIP_KASIA:-0}"
SKIP_CARGO="${SKIP_CARGO:-0}"
SKIP_PACKAGE="${SKIP_PACKAGE:-0}"
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
  --debug               Enable shell trace (default)
  --no-debug            Disable shell trace
  -h, --help            Show this help

Environment equivalents:
  LOG_DIR, ARTIFACT_ROOT, SKIP_KASIA=1, SKIP_CARGO=1, SKIP_PACKAGE=1, DEBUG=0|1
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

sync_external_repo() {
  local dir="$1"
  local url="$2"
  local target="$ROOT_DIR/$dir"

  if [[ -d "$target/.git" ]]; then
    local current_url
    current_url="$(git -C "$target" remote get-url origin 2>/dev/null || true)"
    if [[ -n "$current_url" && "$current_url" != "$url" ]]; then
      echo "External repo remote mismatch for $dir; recloning ($current_url -> $url)"
      nuke_dir "$target"
      git clone --depth 1 "$url" "$target"
      return 0
    fi
    echo "Updating external repo $dir via git pull --ff-only"
    git -C "$target" pull --ff-only
    return 0
  fi

  if [[ -e "$target" ]]; then
    echo "External repo $dir exists without .git; recloning"
    nuke_dir "$target"
  fi

  echo "Cloning external repo $dir"
  git clone --depth 1 "$url" "$target"
}

sync_external_repos() {
  KASPA_NG_EXTERNAL_SYNC_STRICT="${KASPA_NG_EXTERNAL_SYNC_STRICT:-1}" \
  KASPA_NG_EXTERNAL_SYNC_RETRIES="${KASPA_NG_EXTERNAL_SYNC_RETRIES:-4}" \
    bash "$ROOT_DIR/scripts/sync-external-repos.sh" "$ROOT_DIR"
}

nuke_dir() {
  local dir="$1"
  local attempt
  if [[ ! -e "$dir" ]]; then
    return 0
  fi

  chmod -R u+w "$dir" 2>/dev/null || true
  for attempt in 1 2 3 4 5; do
    rm -rf "$dir" 2>/dev/null || true
    [[ ! -e "$dir" ]] && return 0
    find "$dir" -name '.DS_Store' -type f -delete 2>/dev/null || true
    sleep 0.2
  done

  echo "Failed to remove directory: $dir" >&2
  return 1
}

resign_native_nodes() {
  [[ "$(uname -s)" == "Darwin" ]] || return 0
  command -v codesign >/dev/null 2>&1 || return 0

  local search_dir="$1"
  [[ -d "$search_dir" ]] || return 0

  local node_file
  while IFS= read -r -d '' node_file; do
    codesign --force --sign - "$node_file" >/dev/null 2>&1 || true
  done < <(find "$search_dir" -type f -name '*.node' -print0 2>/dev/null)
}

ensure_rollup_native() {
  [[ -f node_modules/rollup/dist/native.js ]] || return 0

  if node -e "require('rollup/dist/native.js')" >/dev/null 2>&1; then
    return 0
  fi

  local os arch pkg
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os/$arch" in
    Linux/x86_64)
      pkg="@rollup/rollup-linux-x64-gnu"
      ;;
    Linux/aarch64|Linux/arm64)
      pkg="@rollup/rollup-linux-arm64-gnu"
      ;;
    *)
      echo "rollup native binding missing; unsupported auto-fix target: $os/$arch" >&2
      return 1
      ;;
  esac

  echo "rollup native binding missing; installing ${pkg}"
  npm install --no-audit --no-fund --no-save "$pkg"
  node -e "require('rollup/dist/native.js')" >/dev/null
}

ensure_swc_native() {
  [[ -d node_modules/@swc/core ]] || return 0

  if node -e "require('@swc/core')" >/dev/null 2>&1; then
    return 0
  fi

  local os arch pkg
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os/$arch" in
    Darwin/arm64)
      pkg="@swc/core-darwin-arm64"
      ;;
    Darwin/x86_64)
      pkg="@swc/core-darwin-x64"
      ;;
    Linux/x86_64)
      pkg="@swc/core-linux-x64-gnu"
      ;;
    Linux/aarch64|Linux/arm64)
      pkg="@swc/core-linux-arm64-gnu"
      ;;
    *)
      echo "swc native binding missing; unsupported auto-fix target: $os/$arch" >&2
      return 1
      ;;
  esac

  echo "swc native binding missing; reinstalling @swc/core and ${pkg}"
  npm install --no-audit --no-fund --no-save @swc/core "$pkg"
  resign_native_nodes "node_modules/@swc"
  node -e "require('@swc/core')" >/dev/null
}

ensure_tailwind_oxide_native() {
  [[ -d node_modules/@tailwindcss/oxide ]] || return 0

  if node -e "require('@tailwindcss/oxide')" >/dev/null 2>&1; then
    return 0
  fi

  local os arch pkg
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os/$arch" in
    Darwin/arm64)
      pkg="@tailwindcss/oxide-darwin-arm64"
      ;;
    Darwin/x86_64)
      pkg="@tailwindcss/oxide-darwin-x64"
      ;;
    Linux/x86_64)
      pkg="@tailwindcss/oxide-linux-x64-gnu"
      ;;
    Linux/aarch64|Linux/arm64)
      pkg="@tailwindcss/oxide-linux-arm64-gnu"
      ;;
    *)
      echo "tailwindcss oxide native binding missing; unsupported auto-fix target: $os/$arch" >&2
      return 1
      ;;
  esac

  echo "tailwindcss oxide native binding missing; reinstalling @tailwindcss/oxide and ${pkg}"
  npm install --no-audit --no-fund --no-save @tailwindcss/oxide "$pkg"
  resign_native_nodes "node_modules/@tailwindcss"
  node -e "require('@tailwindcss/oxide')" >/dev/null
}

ensure_js_native_bundle() {
  local os arch swc_pkg tailwind_pkg
  local pkgs=()
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os/$arch" in
    Darwin/arm64)
      swc_pkg="@swc/core-darwin-arm64"
      tailwind_pkg="@tailwindcss/oxide-darwin-arm64"
      ;;
    Darwin/x86_64)
      swc_pkg="@swc/core-darwin-x64"
      tailwind_pkg="@tailwindcss/oxide-darwin-x64"
      ;;
    Linux/x86_64)
      swc_pkg="@swc/core-linux-x64-gnu"
      tailwind_pkg="@tailwindcss/oxide-linux-x64-gnu"
      ;;
    Linux/aarch64|Linux/arm64)
      swc_pkg="@swc/core-linux-arm64-gnu"
      tailwind_pkg="@tailwindcss/oxide-linux-arm64-gnu"
      ;;
    *)
      return 0
      ;;
  esac

  if [[ -d node_modules/@swc/core ]]; then
    pkgs+=("@swc/core" "$swc_pkg")
  fi
  if [[ -d node_modules/@tailwindcss/oxide ]]; then
    pkgs+=("@tailwindcss/oxide" "$tailwind_pkg")
  fi

  if [[ ${#pkgs[@]} -eq 0 ]]; then
    return 0
  fi

  echo "ensuring native JS binding bundle: ${pkgs[*]}"
  npm install --no-audit --no-fund --no-save "${pkgs[@]}"
  resign_native_nodes "node_modules/@swc"
  resign_native_nodes "node_modules/@tailwindcss"

  [[ -d node_modules/@swc/core ]] && node -e "require('@swc/core')" >/dev/null
  [[ -d node_modules/@tailwindcss/oxide ]] && node -e "require('@tailwindcss/oxide')" >/dev/null
}

npm_install_with_fallback() {
  local install_ok=0

  if [[ -f package-lock.json ]]; then
    npm ci --prefer-offline --no-audit --no-fund && install_ok=1 || true
    if [[ "$install_ok" != "1" ]]; then
      echo "npm ci failed; resetting node_modules and retrying npm install"
      nuke_dir node_modules
      npm install --no-audit --no-fund && install_ok=1
    fi
  else
    npm install --no-audit --no-fund && install_ok=1 || true
    if [[ "$install_ok" != "1" ]]; then
      echo "npm install failed; resetting node_modules and retrying once"
      nuke_dir node_modules
      npm install --no-audit --no-fund && install_ok=1
    fi
  fi

  ensure_rollup_native
  ensure_js_native_bundle
  ensure_tailwind_oxide_native
  ensure_swc_native
}

ensure_wasm_pack() {
  if command -v wasm-pack >/dev/null 2>&1; then
    return 0
  fi

  echo "wasm-pack not found; installing via cargo"
  cargo install wasm-pack --locked
  command -v wasm-pack >/dev/null 2>&1 || {
    echo "Failed to install wasm-pack" >&2
    exit 1
  }
}

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
  ensure_wasm_pack
  (
    cd Kasia
    export BROWSERSLIST_IGNORE_OLD_DATA=1
    npm_install_with_fallback
    npm run wasm:build \
      2> >(grep -vF "[WARN  wasm_pack::install] could not download pre-built \`wasm-bindgen\`" >&2 || true)
    npm run build:production -- --logLevel error \
      2> >(grep -vF "[baseline-browser-mapping]" >&2 || true) \
      || npm exec vite build -- --logLevel error \
        2> >(grep -vF "[baseline-browser-mapping]" >&2 || true)
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
    npm_install_with_fallback
    npm run build
  )
}

build_release() {
  KASIA_WASM_AUTO_FETCH=0 cargo build --release
}

stage_postgres_runtime() {
  local stage_script out_dir
  stage_script="$ROOT_DIR/scripts/stage-postgres-runtime.sh"
  out_dir="$ROOT_DIR/target/release/postgres"
  [[ -f "$stage_script" ]] || return 0
  bash "$stage_script" "$out_dir"
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
  elif [[ -f "simply-kaspa-indexer/target/release/${bin}" ]]; then
    cp "simply-kaspa-indexer/target/release/${bin}" "$dest/"
  elif [[ -f "K-indexer/target/release/${bin}" ]]; then
    cp "K-indexer/target/release/${bin}" "$dest/"
  elif [[ "$bin" == "kasia-indexer" && -f "kasia-indexer/target/release/indexer" ]]; then
    cp "kasia-indexer/target/release/indexer" "$dest/${bin}"
  fi
}

copy_dir_filtered() {
  local src="$1"
  local dst="$2"
  [[ -d "$src" ]] || return 0
  mkdir -p "$dst"
  (
    cd "$src"
    tar \
      --exclude='.venv' \
      --exclude='__pycache__' \
      --exclude='.DS_Store' \
      -cf - .
  ) | (
    cd "$dst"
    tar -xf -
  )
}

package_and_verify() {
  local short_sha platform root os tarball
  short_sha="$(git rev-parse --short HEAD 2>/dev/null || echo "local")"
  platform="$(detect_platform_suffix)"
  root="${ARTIFACT_ROOT:-kaspa-ng-${short_sha}-${platform}-local-sim}"
  os="$(uname -s)"
  tarball=""

  nuke_dir "$root"
  mkdir -p "$root"

  build_explorer_if_missing

  if [[ "$os" == "Darwin" ]]; then
    KASPA_NG_SKIP_EXTERNAL_SYNC=1 "$ROOT_DIR/scripts/macos-bundle.sh" release
    [[ -d "$ROOT_DIR/target/release/Kaspa-NG.app" ]] || {
      echo "Missing macOS app bundle: target/release/Kaspa-NG.app" >&2
      exit 1
    }
    cp -R "$ROOT_DIR/target/release/Kaspa-NG.app" "$root/"
  fi

  cp target/release/kaspa-ng "$root/"

  if [[ "$os" == "Linux" ]]; then
    cp core/resources/icons/icon-256.png "$root/kaspa-ng.png"
    cp core/resources/packaging/kaspa-ng.desktop "$root/"
  fi

  local bin
  for bin in stratum-bridge simply-kaspa-indexer K-webserver K-transaction-processor kasia-indexer; do
    copy_binary_if_exists "$bin" "$root"
  done

  if [[ -d target/release/kaspa-explorer-ng ]]; then
    cp -r target/release/kaspa-explorer-ng "$root/"
  elif [[ -d kaspa-explorer-ng/build ]]; then
    mkdir -p "$root/kaspa-explorer-ng"
    cp -r kaspa-explorer-ng/build "$root/kaspa-explorer-ng/"
  fi

  copy_dir_filtered "kaspa-rest-server" "$root/kaspa-rest-server"
  copy_dir_filtered "kaspa-socket-server" "$root/kaspa-socket-server"
  copy_dir_filtered "Loader" "$root/Loader"

  if [[ ! -d target/release/postgres ]]; then
    echo "Missing staged PostgreSQL runtime: target/release/postgres" >&2
    exit 1
  fi
  cp -r target/release/postgres "$root/postgres"

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

  for bin in kaspa-ng stratum-bridge simply-kaspa-indexer K-webserver K-transaction-processor kasia-indexer; do
    [[ -f "$root/$bin" ]] || { echo "Missing packaged binary: $bin" >&2; exit 1; }
  done
  for dir in kaspa-explorer-ng kaspa-rest-server kaspa-socket-server Loader K Kasia KasVault postgres; do
    [[ -d "$root/$dir" ]] || { echo "Missing packaged directory: $dir" >&2; exit 1; }
  done
  bash "$ROOT_DIR/scripts/verify-self-hosted-python-runtime.sh" "$root"
  if [[ ! -x "$root/postgres/bin/postgres" && ! -x "$root/postgres/bin/postgres.exe" ]]; then
    echo "Missing PostgreSQL runtime binary in packaged layout (postgres)" >&2
    exit 1
  fi
  if [[ ! -x "$root/postgres/bin/initdb" && ! -x "$root/postgres/bin/initdb.exe" ]]; then
    echo "Missing PostgreSQL runtime binary in packaged layout (initdb)" >&2
    exit 1
  fi
  if [[ ! -x "$root/postgres/bin/pg_ctl" && ! -x "$root/postgres/bin/pg_ctl.exe" ]]; then
    echo "Missing PostgreSQL runtime binary in packaged layout (pg_ctl)" >&2
    exit 1
  fi
  if [[ "$os" == "Darwin" ]]; then
    [[ -d "$root/Kaspa-NG.app" ]] || { echo "Missing packaged app bundle: Kaspa-NG.app" >&2; exit 1; }
    [[ -f "$root/Kaspa-NG.app/Contents/Info.plist" ]] || { echo "Missing app Info.plist" >&2; exit 1; }
    [[ -f "$root/Kaspa-NG.app/Contents/MacOS/kaspa-ng" ]] || { echo "Missing app executable" >&2; exit 1; }
  fi

  if [[ "$os" == "Linux" ]]; then
    tarball="${root}.tar.gz"
    rm -f "$tarball"
    tar -czf "$tarball" "$root"
  fi

  echo "LOCAL_ARTIFACT_SIM_OK root=$root tarball=${tarball:-none}"
}

echo "==> [0/5] Sync external repositories"
sync_external_repos 2>&1 | tee "$LOG_DIR/external-repo-sync.log"
bash "$ROOT_DIR/scripts/patch-rusty-kaspa-workflow-perf-monitor.sh" 2>&1 | tee "$LOG_DIR/rusty-kaspa-deps-patch.log"

echo "==> [1/5] Prepare Kasia wasm package"
prepare_kasia_wasm 2>&1 | tee "$LOG_DIR/prepare-kasia-wasm.log"

if [[ "$SKIP_KASIA" != "1" ]]; then
  echo "==> [2/5] Build Kasia frontend"
  build_kasia 2>&1 | tee "$LOG_DIR/kasia-build.log"
else
  echo "==> [2/5] Skipped Kasia build"
fi

if [[ "$SKIP_CARGO" != "1" ]]; then
  echo "==> [3/5] Cargo release build"
  build_release 2>&1 | tee "$LOG_DIR/cargo-build-release.log"
  echo "==> [3b/5] Stage internal PostgreSQL runtime"
  stage_postgres_runtime 2>&1 | tee "$LOG_DIR/postgres-runtime-stage.log"
else
  echo "==> [3/5] Skipped cargo build"
fi

if [[ "$SKIP_PACKAGE" != "1" ]]; then
  echo "==> [4/5] Package + verify artifact layout"
  package_and_verify 2>&1 | tee "$LOG_DIR/package-verify.log"
else
  echo "==> [4/5] Skipped package/verify"
fi

echo "Done. Logs in: $LOG_DIR"
