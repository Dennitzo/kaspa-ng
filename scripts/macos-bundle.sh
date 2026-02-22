#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${1:-release}"
APP_NAME="Kaspa-NG"

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

# Bundle K-Social frontend assets if present.
if [ -d "${ROOT}/K/dist" ]; then
  mkdir -p "$MACOS_DIR/K"
  cp -r "${ROOT}/K/dist" "$MACOS_DIR/K/"
elif [ -d "${ROOT}/target/${PROFILE}/K/dist" ]; then
  mkdir -p "$MACOS_DIR/K"
  cp -r "${ROOT}/target/${PROFILE}/K/dist" "$MACOS_DIR/K/"
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
  <string>10.13</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF

echo "Created ${APP_DIR}"
echo "Run: open \"${APP_DIR}\""
