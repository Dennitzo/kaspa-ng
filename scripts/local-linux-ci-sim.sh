#!/usr/bin/env bash

set -Eeuo pipefail
IFS=$'\n\t'

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

IMAGE="${IMAGE:-ubuntu:22.04}"
PLATFORM="${PLATFORM:-linux/amd64}"
CONTAINER_WORKDIR="/workspace"
ARTIFACT_ROOT="${ARTIFACT_ROOT:-}"
SKIP_APPIMAGE="${SKIP_APPIMAGE:-0}"
DEBUG="${DEBUG:-1}"
FORCE_APPIMAGE="${FORCE_APPIMAGE:-0}"

usage() {
  cat <<'USAGE'
Run a Linux (ubuntu-22.04) CI-like artifact build inside Docker/Colima.

Usage:
  scripts/local-linux-ci-sim.sh [options]

Options:
  --image <name>            Docker image (default: ubuntu:22.04)
  --platform <platform>     Docker platform (default: linux/amd64)
  --artifact-root <name>    Optional artifact root name override
  --skip-appimage           Skip AppImage step in local-artifact-sim
  --force-appimage          Force AppImage step even on hosts where it is auto-skipped
  --debug                   Enable shell trace (default)
  --no-debug                Disable shell trace
  -h, --help                Show this help

Environment equivalents:
  IMAGE, PLATFORM, ARTIFACT_ROOT, SKIP_APPIMAGE=0|1, FORCE_APPIMAGE=0|1, DEBUG=0|1
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --image)
      IMAGE="$2"
      shift 2
      ;;
    --platform)
      PLATFORM="$2"
      shift 2
      ;;
    --artifact-root)
      ARTIFACT_ROOT="$2"
      shift 2
      ;;
    --skip-appimage)
      SKIP_APPIMAGE=1
      shift
      ;;
    --debug)
      DEBUG=1
      shift
      ;;
    --force-appimage)
      FORCE_APPIMAGE=1
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

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Required command not found: $1" >&2
    exit 1
  }
}

require_cmd docker

if [[ "$DEBUG" == "1" ]]; then
  export PS4='+ [${BASH_SOURCE##*/}:${LINENO}] '
  set -x
fi

# AppImage generation via linuxdeploy often fails under Docker emulation on macOS
# with "Exec format error". Default to skipping it there unless explicitly forced.
if [[ "$(uname -s)" == "Darwin" && "$FORCE_APPIMAGE" != "1" && "$SKIP_APPIMAGE" != "1" ]]; then
  echo "Host is macOS; auto-enabling --skip-appimage for Docker Linux simulation."
  echo "Use --force-appimage to attempt AppImage generation anyway."
  SKIP_APPIMAGE=1
fi

CONTAINER_SCRIPT="$(mktemp)"
trap 'rm -f "$CONTAINER_SCRIPT"' EXIT

cat > "$CONTAINER_SCRIPT" <<'INNER'
#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates curl git build-essential pkg-config \
  libssl-dev libglib2.0-dev libatk1.0-dev libgtk-4-dev \
  libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev libx11-dev protobuf-compiler libprotobuf-dev \
  python3 python3-pip zip xz-utils clang libclang-dev llvm-dev

# Node.js 22 (matches CI)
curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
apt-get install -y nodejs

# Rust stable toolchain
curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal
source "$HOME/.cargo/env"
rustup toolchain install stable
rustup default stable

# Ensure bindgen can find libclang for crates like librocksdb-sys.
if command -v llvm-config >/dev/null 2>&1; then
  export LIBCLANG_PATH="$(llvm-config --libdir)"
fi

cd /workspace
chmod +x scripts/local-artifact-sim.sh

sim_args=(--no-debug)
if [[ "${SKIP_APPIMAGE:-0}" == "1" ]]; then
  sim_args+=(--no-appimage)
else
  sim_args+=(--build-appimage)
fi
if [[ -n "${ARTIFACT_ROOT:-}" ]]; then
  sim_args+=(--artifact-root "$ARTIFACT_ROOT")
fi

./scripts/local-artifact-sim.sh "${sim_args[@]}"
INNER

chmod +x "$CONTAINER_SCRIPT"

DOCKER_ENV=(
  -e SKIP_APPIMAGE="$SKIP_APPIMAGE"
)
if [[ -n "$ARTIFACT_ROOT" ]]; then
  DOCKER_ENV+=( -e ARTIFACT_ROOT="$ARTIFACT_ROOT" )
fi

docker run --rm -t \
  --platform "$PLATFORM" \
  -v "$ROOT_DIR":"$CONTAINER_WORKDIR" \
  -v "$CONTAINER_SCRIPT":"/tmp/local-linux-ci-inner.sh:ro" \
  -w "$CONTAINER_WORKDIR" \
  "${DOCKER_ENV[@]}" \
  "$IMAGE" \
  bash /tmp/local-linux-ci-inner.sh
