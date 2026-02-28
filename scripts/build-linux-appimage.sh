#!/usr/bin/env bash

set -Eeuo pipefail
IFS=$'\n\t'

usage() {
  cat <<'EOF'
Build Kaspa-NG AppImage from a prepared Linux package directory.

Usage:
  scripts/build-linux-appimage.sh --input <dir> --output <file.AppImage>

Input directory must contain at least:
  - kaspa-ng (binary)
  - kaspa-ng.desktop
  - kaspa-ng.png
  - bundled service/data directories used by kaspa-ng
EOF
}

INPUT_DIR=""
OUTPUT_FILE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --input)
      INPUT_DIR="$2"
      shift 2
      ;;
    --output)
      OUTPUT_FILE="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

[[ -n "$INPUT_DIR" ]] || { echo "--input is required" >&2; exit 2; }
[[ -n "$OUTPUT_FILE" ]] || { echo "--output is required" >&2; exit 2; }
[[ -d "$INPUT_DIR" ]] || { echo "Input directory not found: $INPUT_DIR" >&2; exit 1; }
[[ -f "$INPUT_DIR/kaspa-ng" ]] || { echo "Missing binary: $INPUT_DIR/kaspa-ng" >&2; exit 1; }
[[ -f "$INPUT_DIR/kaspa-ng.desktop" ]] || { echo "Missing desktop file: $INPUT_DIR/kaspa-ng.desktop" >&2; exit 1; }
[[ -f "$INPUT_DIR/kaspa-ng.png" ]] || { echo "Missing icon: $INPUT_DIR/kaspa-ng.png" >&2; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Required command not found: $1" >&2
    exit 1
  }
}

require_cmd curl
require_cmd sed
require_cmd find

download_with_fallback() {
  local out="$1"
  shift
  local url
  for url in "$@"; do
    if curl -fL --retry 3 -o "$out" "$url"; then
      return 0
    fi
  done
  return 1
}

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

APPDIR="$WORK_DIR/AppDir"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"

# Keep kaspa-ng runtime layout identical to existing package by copying all files into usr/bin.
cp -a "$INPUT_DIR"/. "$APPDIR/usr/bin/"

# Ensure internal PostgreSQL runtime is present in the AppImage payload.
if [[ ! -x "$APPDIR/usr/bin/postgres/bin/postgres" ]]; then
  STAGED_PG="$WORK_DIR/postgres"
  if [[ -f "$SCRIPT_DIR/stage-postgres-runtime.sh" ]]; then
    if bash "$SCRIPT_DIR/stage-postgres-runtime.sh" "$STAGED_PG"; then
      mkdir -p "$APPDIR/usr/bin/postgres"
      cp -a "$STAGED_PG"/. "$APPDIR/usr/bin/postgres/"
    fi
  fi
fi

cp "$INPUT_DIR/kaspa-ng.desktop" "$APPDIR/usr/share/applications/kaspa-ng.desktop"
cp "$INPUT_DIR/kaspa-ng.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/kaspa-ng.png"

# Ensure desktop entry launches the main binary.
sed -i 's|^Exec=.*|Exec=kaspa-ng|g' "$APPDIR/usr/share/applications/kaspa-ng.desktop"
sed -i 's|^Icon=.*|Icon=kaspa-ng|g' "$APPDIR/usr/share/applications/kaspa-ng.desktop"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
set -eu
HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export PATH="$HERE/usr/bin:$PATH"
cd "$HERE/usr/bin"
exec "$HERE/usr/bin/kaspa-ng" "$@"
EOF
chmod +x "$APPDIR/AppRun"

LINUXDEPLOY="$WORK_DIR/linuxdeploy-x86_64.AppImage"
GTK_PLUGIN="$WORK_DIR/linuxdeploy-plugin-gtk.sh"
download_with_fallback "$LINUXDEPLOY" \
  "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage" \
  "https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20250213-2/linuxdeploy-x86_64.AppImage" \
  || { echo "Failed to download linuxdeploy" >&2; exit 1; }
download_with_fallback "$GTK_PLUGIN" \
  "https://github.com/linuxdeploy/linuxdeploy-plugin-gtk/releases/download/continuous/linuxdeploy-plugin-gtk.sh" \
  "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/main/linuxdeploy-plugin-gtk.sh" \
  "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh" \
  || { echo "Failed to download linuxdeploy GTK plugin" >&2; exit 1; }
chmod +x "$LINUXDEPLOY" "$GTK_PLUGIN"

export ARCH=x86_64
export NO_APPSTREAM=1
export OUTPUT="$WORK_DIR"
export APPIMAGE_EXTRACT_AND_RUN=1

"$LINUXDEPLOY" \
  --appdir "$APPDIR" \
  --plugin gtk \
  --desktop-file "$APPDIR/usr/share/applications/kaspa-ng.desktop" \
  --icon-file "$APPDIR/usr/share/icons/hicolor/256x256/apps/kaspa-ng.png" \
  --executable "$APPDIR/usr/bin/kaspa-ng" \
  --output appimage

APPIMAGE_FILE="$(find "$WORK_DIR" -maxdepth 1 -type f -name '*.AppImage' | head -n 1 || true)"
[[ -n "$APPIMAGE_FILE" ]] || { echo "linuxdeploy did not produce an AppImage" >&2; exit 1; }

mkdir -p "$(dirname "$OUTPUT_FILE")"
cp "$APPIMAGE_FILE" "$OUTPUT_FILE"
chmod +x "$OUTPUT_FILE"
echo "AppImage created: $OUTPUT_FILE"
