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

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

APPDIR="$WORK_DIR/AppDir"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"

# Keep kaspa-ng runtime layout identical to existing package by copying all files into usr/bin.
cp -a "$INPUT_DIR"/. "$APPDIR/usr/bin/"

cp "$INPUT_DIR/kaspa-ng.desktop" "$APPDIR/usr/share/applications/kaspa-ng.desktop"
cp "$INPUT_DIR/kaspa-ng.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/kaspa-ng.png"

# Ensure desktop entry launches through wrapper that sets cwd to bundled directory.
sed -i 's|^Exec=.*|Exec=kaspa-ng-launch|g' "$APPDIR/usr/share/applications/kaspa-ng.desktop"
sed -i 's|^Icon=.*|Icon=kaspa-ng|g' "$APPDIR/usr/share/applications/kaspa-ng.desktop"

cat > "$APPDIR/usr/bin/kaspa-ng-launch" <<'EOF'
#!/bin/sh
set -eu
SELF_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$SELF_DIR"
exec "$SELF_DIR/kaspa-ng" "$@"
EOF
chmod +x "$APPDIR/usr/bin/kaspa-ng-launch"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
set -eu
HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/kaspa-ng-launch" "$@"
EOF
chmod +x "$APPDIR/AppRun"

LINUXDEPLOY="$WORK_DIR/linuxdeploy-x86_64.AppImage"
GTK_PLUGIN="$WORK_DIR/linuxdeploy-plugin-gtk.sh"
curl -fL --retry 3 -o "$LINUXDEPLOY" \
  "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
curl -fL --retry 3 -o "$GTK_PLUGIN" \
  "https://github.com/linuxdeploy/linuxdeploy-plugin-gtk/releases/download/continuous/linuxdeploy-plugin-gtk.sh"
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
  --executable "$APPDIR/usr/bin/kaspa-ng-launch" \
  --output appimage

APPIMAGE_FILE="$(find "$WORK_DIR" -maxdepth 1 -type f -name '*.AppImage' | head -n 1 || true)"
[[ -n "$APPIMAGE_FILE" ]] || { echo "linuxdeploy did not produce an AppImage" >&2; exit 1; }

mkdir -p "$(dirname "$OUTPUT_FILE")"
cp "$APPIMAGE_FILE" "$OUTPUT_FILE"
chmod +x "$OUTPUT_FILE"
echo "AppImage created: $OUTPUT_FILE"
