#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${1:-release}"
APP_NAME="Kaspa-NG"
MACOS_MIN_VERSION="${MACOS_MIN_VERSION:-12.0}"
POSTGRES_STAGE_SCRIPT="${ROOT}/scripts/stage-postgres-runtime.sh"
PYTHON_STAGE_SCRIPT="${ROOT}/scripts/stage-python-runtime.sh"
SYNC_EXTERNAL="${KASPA_NG_SKIP_EXTERNAL_SYNC:-0}"

BIN="${ROOT}/target/${PROFILE}/kaspa-ng"
if [ ! -f "$BIN" ]; then
  echo "kaspa-ng binary not found at ${BIN}"
  echo "Run: cargo build --release"
  exit 1
fi

APP_DIR="${ROOT}/target/${PROFILE}/${APP_NAME}.app"
MACOS_DIR="${APP_DIR}/Contents/MacOS"
RES_DIR="${APP_DIR}/Contents/Resources"

copy_file_if_exists() {
  local src="$1"
  local dst="$2"
  if [ -f "$src" ]; then
    mkdir -p "$(dirname "$dst")"
    cp "$src" "$dst"
    chmod +x "$dst" 2>/dev/null || true
    return 0
  fi
  return 1
}

copy_dir_if_exists() {
  local src="$1"
  local dst="$2"
  if [ -d "$src" ]; then
    mkdir -p "$(dirname "$dst")"
    rm -rf "$dst"
    mkdir -p "$dst"
    (
      cd "$src"
      tar \
        --exclude='.git' \
        --exclude='.github' \
        --exclude='.venv' \
        --exclude='__pycache__' \
        --exclude='.DS_Store' \
        -cf - .
    ) | (
      cd "$dst"
      tar -xf -
    )
    return 0
  fi
  return 1
}

sync_external_repos_if_needed() {
  if [ "${SYNC_EXTERNAL}" = "1" ] || [ "${SYNC_EXTERNAL}" = "true" ] || [ "${SYNC_EXTERNAL}" = "TRUE" ]; then
    echo "Skipping external repo sync (KASPA_NG_SKIP_EXTERNAL_SYNC=${SYNC_EXTERNAL})"
    return 0
  fi

  if ! command -v git >/dev/null 2>&1; then
    echo "git not found; skipping external repo sync"
    return 0
  fi

  # Bundling happens after a successful build; a stale existing checkout is acceptable
  # if remote endpoints are temporarily unavailable.
  KASPA_NG_EXTERNAL_SYNC_STRICT="${KASPA_NG_EXTERNAL_SYNC_STRICT:-0}" \
  KASPA_NG_EXTERNAL_SYNC_RETRIES="${KASPA_NG_EXTERNAL_SYNC_RETRIES:-4}" \
    bash "${ROOT}/scripts/sync-external-repos.sh" "${ROOT}"
}

sync_external_repos_if_needed
bash "${ROOT}/scripts/patch-rusty-kaspa-workflow-perf-monitor.sh"

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RES_DIR"
cp "$BIN" "$MACOS_DIR/"
chmod +x "$MACOS_DIR/kaspa-ng" 2>/dev/null || true

if [ -f "${ROOT}/target/${PROFILE}/stratum-bridge" ]; then
  cp "${ROOT}/target/${PROFILE}/stratum-bridge" "$MACOS_DIR/"
elif [ -f "${ROOT}/rusty-kaspa/target/${PROFILE}/stratum-bridge" ]; then
  cp "${ROOT}/rusty-kaspa/target/${PROFILE}/stratum-bridge" "$MACOS_DIR/"
fi


if [ -d "${ROOT}/target/${PROFILE}/kaspa-explorer-ng" ]; then
  cp -r "${ROOT}/target/${PROFILE}/kaspa-explorer-ng" "$MACOS_DIR/"
elif [ -d "${ROOT}/kaspa-explorer-ng/build" ]; then
  mkdir -p "$MACOS_DIR/kaspa-explorer-ng"
  cp -r "${ROOT}/kaspa-explorer-ng/build" "$MACOS_DIR/kaspa-explorer-ng/"
fi

# Bundle explorer backend servers used by self-hosted REST/socket services.
copy_dir_if_exists \
  "${ROOT}/kaspa-rest-server" \
  "${MACOS_DIR}/kaspa-rest-server" || true
copy_dir_if_exists \
  "${ROOT}/kaspa-socket-server" \
  "${MACOS_DIR}/kaspa-socket-server" || true
copy_dir_if_exists \
  "${ROOT}/Loader" \
  "${MACOS_DIR}/Loader" || true

# Bundle K-Social frontend assets if present.
if [ -d "${ROOT}/K/dist" ]; then
  mkdir -p "$MACOS_DIR/K"
  cp -r "${ROOT}/K/dist" "$MACOS_DIR/K/"
elif [ -d "${ROOT}/target/${PROFILE}/K/dist" ]; then
  mkdir -p "$MACOS_DIR/K"
  cp -r "${ROOT}/target/${PROFILE}/K/dist" "$MACOS_DIR/K/"
fi

# Bundle Kasia frontend assets if present.
if [ -d "${ROOT}/Kasia/dist" ]; then
  mkdir -p "$MACOS_DIR/Kasia"
  cp -r "${ROOT}/Kasia/dist" "$MACOS_DIR/Kasia/"
elif [ -d "${ROOT}/target/${PROFILE}/Kasia/dist" ]; then
  mkdir -p "$MACOS_DIR/Kasia"
  cp -r "${ROOT}/target/${PROFILE}/Kasia/dist" "$MACOS_DIR/Kasia/"
fi

# Bundle KasVault frontend assets if present.
if [ -d "${ROOT}/kasvault/build" ]; then
  mkdir -p "$MACOS_DIR/KasVault"
  cp -r "${ROOT}/kasvault/build" "$MACOS_DIR/KasVault/"
elif [ -d "${ROOT}/target/${PROFILE}/KasVault/build" ]; then
  mkdir -p "$MACOS_DIR/KasVault"
  cp -r "${ROOT}/target/${PROFILE}/KasVault/build" "$MACOS_DIR/KasVault/"
fi

# Bundle self-hosted indexer binaries in paths expected by runtime lookup.
copy_file_if_exists \
  "${ROOT}/simply-kaspa-indexer/target/${PROFILE}/simply-kaspa-indexer" \
  "${MACOS_DIR}/simply-kaspa-indexer/target/${PROFILE}/simply-kaspa-indexer" || true
copy_file_if_exists \
  "${ROOT}/target/${PROFILE}/simply-kaspa-indexer" \
  "${MACOS_DIR}/simply-kaspa-indexer/target/${PROFILE}/simply-kaspa-indexer" || true

copy_file_if_exists \
  "${ROOT}/K-indexer/target/${PROFILE}/K-transaction-processor" \
  "${MACOS_DIR}/K-indexer/target/${PROFILE}/K-transaction-processor" || true
copy_file_if_exists \
  "${ROOT}/K-indexer/target/${PROFILE}/K-webserver" \
  "${MACOS_DIR}/K-indexer/target/${PROFILE}/K-webserver" || true

copy_file_if_exists \
  "${ROOT}/target/${PROFILE}/K-transaction-processor" \
  "${MACOS_DIR}/K-indexer/target/${PROFILE}/K-transaction-processor" || true
copy_file_if_exists \
  "${ROOT}/target/${PROFILE}/K-webserver" \
  "${MACOS_DIR}/K-indexer/target/${PROFILE}/K-webserver" || true

copy_file_if_exists \
  "${ROOT}/K-indexer/target/${PROFILE}/K-transaction-processor" \
  "${MACOS_DIR}/K-transaction-processor" || true
copy_file_if_exists \
  "${ROOT}/K-indexer/target/${PROFILE}/K-webserver" \
  "${MACOS_DIR}/K-webserver" || true
copy_file_if_exists \
  "${ROOT}/target/${PROFILE}/K-transaction-processor" \
  "${MACOS_DIR}/K-transaction-processor" || true
copy_file_if_exists \
  "${ROOT}/target/${PROFILE}/K-webserver" \
  "${MACOS_DIR}/K-webserver" || true

copy_file_if_exists \
  "${ROOT}/kasia-indexer/target/${PROFILE}/indexer" \
  "${MACOS_DIR}/kasia-indexer/target/${PROFILE}/kasia-indexer" || true
copy_file_if_exists \
  "${ROOT}/target/${PROFILE}/kasia-indexer" \
  "${MACOS_DIR}/kasia-indexer/target/${PROFILE}/kasia-indexer" || true
copy_file_if_exists \
  "${ROOT}/kasia-indexer/target/${PROFILE}/indexer" \
  "${MACOS_DIR}/kasia-indexer" || true
copy_file_if_exists \
  "${ROOT}/target/${PROFILE}/kasia-indexer" \
  "${MACOS_DIR}/kasia-indexer" || true

# Bundle internal PostgreSQL runtime used by self-hosted services.
if [ -f "$POSTGRES_STAGE_SCRIPT" ]; then
  bash "$POSTGRES_STAGE_SCRIPT" "${ROOT}/target/${PROFILE}/postgres"
fi
if [ ! -d "${ROOT}/target/${PROFILE}/postgres" ]; then
  echo "Missing staged PostgreSQL runtime: ${ROOT}/target/${PROFILE}/postgres" >&2
  exit 1
fi
mkdir -p "${RES_DIR}/postgres"
cp -R "${ROOT}/target/${PROFILE}/postgres"/. "${RES_DIR}/postgres/"

# Bundle internal Python runtime used by self-hosted services.
if [ -f "$PYTHON_STAGE_SCRIPT" ]; then
  bash "$PYTHON_STAGE_SCRIPT" "${ROOT}/target/${PROFILE}/python"
fi
if [ ! -d "${ROOT}/target/${PROFILE}/python" ]; then
  echo "Missing staged Python runtime: ${ROOT}/target/${PROFILE}/python" >&2
  exit 1
fi
mkdir -p "${RES_DIR}/python"
cp -R "${ROOT}/target/${PROFILE}/python"/. "${RES_DIR}/python/"

ICON_SRC="${ROOT}/core/resources/icons/icon-1024.png"
ICONSET="${RES_DIR}/${APP_NAME}.iconset"
mkdir -p "$ICONSET"
sips -z 16 16 "$ICON_SRC" --out "$ICONSET/icon_16x16.png" >/dev/null
sips -z 32 32 "$ICON_SRC" --out "$ICONSET/icon_16x16@2x.png" >/dev/null
sips -z 32 32 "$ICON_SRC" --out "$ICONSET/icon_32x32.png" >/dev/null
sips -z 64 64 "$ICON_SRC" --out "$ICONSET/icon_32x32@2x.png" >/dev/null
sips -z 128 128 "$ICON_SRC" --out "$ICONSET/icon_128x128.png" >/dev/null
sips -z 256 256 "$ICON_SRC" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
sips -z 256 256 "$ICON_SRC" --out "$ICONSET/icon_256x256.png" >/dev/null
sips -z 512 512 "$ICON_SRC" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
sips -z 512 512 "$ICON_SRC" --out "$ICONSET/icon_512x512.png" >/dev/null
sips -z 1024 1024 "$ICON_SRC" --out "$ICONSET/icon_512x512@2x.png" >/dev/null
iconutil -c icns "$ICONSET" -o "$RES_DIR/${APP_NAME}.icns"
rm -rf "$ICONSET"

if [ ! -f "$RES_DIR/${APP_NAME}.icns" ]; then
  echo "Warning: .icns was not generated. Finder icon may appear blank."
fi

VERSION="$(awk -F'\"' '/^version =/ {print $2; exit}' "${ROOT}/Cargo.toml")"
RELEASE_TAG="v${VERSION}"

cat > "${APP_DIR}/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleIdentifier</key>
  <string>org.kaspa.kaspa-ng</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleExecutable</key>
  <string>kaspa-ng</string>
  <key>CFBundleVersion</key>
  <string>${RELEASE_TAG}</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleIconFile</key>
  <string>${APP_NAME}</string>
  <key>LSMinimumSystemVersion</key>
  <string>${MACOS_MIN_VERSION}</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF

# Ad-hoc sign the full app bundle so Gatekeeper doesn't treat it as broken
# due to unsigned/partially-signed nested binaries.
if command -v codesign >/dev/null 2>&1; then
  SIGN_IDENTITY="${MACOS_CODESIGN_IDENTITY:--}"
  codesign --force --deep --sign "${SIGN_IDENTITY}" --timestamp=none "${APP_DIR}"
  codesign --verify --deep --strict --verbose=2 "${APP_DIR}"
fi

echo "Created ${APP_DIR}"
echo "Run: open \"${APP_DIR}\""
