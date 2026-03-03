#!/usr/bin/env bash
set -euo pipefail

# NVIDIA Debian installer based on:
# https://docs.nvidia.com/datacenter/tesla/driver-installation-guide/debian.html
#
# Usage:
#   sudo ./nvidia.sh
#
# Optional env vars:
#   MODULE_FLAVOR=open|proprietary      (default: proprietary)
#   INSTALL_PROFILE=full|desktop|compute (default: full)
#   DRIVER_PIN=<branch-or-version>       (example: 580 or 580.95.05)
#
# Examples:
#   sudo MODULE_FLAVOR=open INSTALL_PROFILE=desktop ./nvidia.sh
#   sudo DRIVER_PIN=580 ./nvidia.sh

if [[ ${EUID:-$(id -u)} -ne 0 ]]; then
  echo "Bitte als root starten: sudo $0"
  exit 1
fi

if [[ ! -r /etc/os-release ]]; then
  echo "Kann /etc/os-release nicht lesen."
  exit 1
fi

# shellcheck disable=SC1091
. /etc/os-release

if [[ "${ID:-}" != "debian" ]]; then
  echo "Dieses Skript ist nur für Debian gedacht (gefunden: ${ID:-unbekannt})."
  exit 1
fi

if ! command -v lspci >/dev/null 2>&1; then
  apt-get update
  apt-get install -y pciutils
fi

gpu_lines="$(lspci -nn 2>/dev/null || true)"
if ! grep -qiE '10de:|NVIDIA' <<<"$gpu_lines"; then
  echo "Keine NVIDIA-GPU erkannt."
  lspci -nnk | grep -EA3 'VGA|3D|Display' || true
  exit 1
fi

arch="$(dpkg --print-architecture)"
case "$arch" in
  amd64)
    repo_arch="x86_64"
    ;;
  arm64)
    repo_arch="sbsa"
    ;;
  *)
    echo "Nicht unterstützte Architektur: ${arch}. Erlaubt: amd64, arm64"
    exit 1
    ;;
esac

distro="debian${VERSION_ID}"
module_flavor="${MODULE_FLAVOR:-proprietary}"
install_profile="${INSTALL_PROFILE:-full}"
driver_pin="${DRIVER_PIN:-}"

echo "Debian: ${PRETTY_NAME:-$ID}"
echo "GPU erkannt: $(grep -iE 'NVIDIA|10de:' <<<"$gpu_lines" | head -n1)"
echo "Repo-Distro: ${distro}, Arch-Pfad: ${repo_arch}"
echo "Module: ${module_flavor}, Profil: ${install_profile}"

if [[ "$module_flavor" != "open" && "$module_flavor" != "proprietary" ]]; then
  echo "Ungültig: MODULE_FLAVOR=${module_flavor} (erlaubt: open|proprietary)"
  exit 1
fi

if [[ "$install_profile" != "full" && "$install_profile" != "desktop" && "$install_profile" != "compute" ]]; then
  echo "Ungültig: INSTALL_PROFILE=${install_profile} (erlaubt: full|desktop|compute)"
  exit 1
fi

echo "[1/6] Voraussetzungen installieren (Kernel-Header, Tools) ..."
apt-get update
apt-get install -y \
  linux-headers-"$(uname -r)" \
  build-essential \
  dkms \
  wget \
  ca-certificates \
  gnupg

echo "[2/6] Debian-Repos (contrib/non-free/non-free-firmware) sicherstellen ..."
for f in /etc/apt/sources.list /etc/apt/sources.list.d/*.list; do
  [[ -f "$f" ]] || continue
  awk '
    BEGIN { OFS=" " }
    /^\s*deb(\s+\[.*\])?\s+/ {
      line=$0
      if (line !~ /contrib/) line=line " contrib"
      if (line !~ /non-free([[:space:]]|$)/) line=line " non-free"
      if (line !~ /non-free-firmware/) line=line " non-free-firmware"
      print line
      next
    }
    { print }
  ' "$f" > "${f}.tmp"
  mv "${f}.tmp" "$f"
done

for f in /etc/apt/sources.list.d/*.sources; do
  [[ -f "$f" ]] || continue
  awk '
    /^Components:/ {
      line=$0
      if (line !~ /contrib/) line=line " contrib"
      if (line !~ /non-free([[:space:]]|$)/) line=line " non-free"
      if (line !~ /non-free-firmware/) line=line " non-free-firmware"
      print line
      next
    }
    { print }
  ' "$f" > "${f}.tmp"
  mv "${f}.tmp" "$f"
done

apt-get update

echo "[3/6] NVIDIA Network Repository (cuda-keyring) einrichten ..."
keyring_url="https://developer.download.nvidia.com/compute/cuda/repos/${distro}/${repo_arch}/cuda-keyring_1.1-1_all.deb"
tmp_deb="/tmp/cuda-keyring_1.1-1_all.deb"

wget -O "$tmp_deb" "$keyring_url"
dpkg -i "$tmp_deb"
apt-get update

if [[ -n "$driver_pin" ]]; then
  echo "[4/6] Optionales Version-Locking installieren: ${driver_pin} ..."
  apt-get install -y "nvidia-driver-pinning-${driver_pin}"
else
  echo "[4/6] Kein Version-Locking gesetzt (DRIVER_PIN leer)."
fi

case "${install_profile}:${module_flavor}" in
  full:open)
    pkgs=(nvidia-open)
    ;;
  full:proprietary)
    pkgs=(cuda-drivers)
    ;;
  desktop:open)
    pkgs=(nvidia-driver nvidia-kernel-open-dkms)
    ;;
  desktop:proprietary)
    pkgs=(nvidia-driver nvidia-kernel-dkms)
    ;;
  compute:open)
    pkgs=(nvidia-driver-cuda nvidia-kernel-open-dkms)
    ;;
  compute:proprietary)
    pkgs=(nvidia-driver-cuda nvidia-kernel-dkms)
    ;;
  *)
    echo "Interner Fehler bei Paketauflösung: ${install_profile}:${module_flavor}"
    exit 1
    ;;
esac

echo "[5/6] NVIDIA-Pakete installieren: ${pkgs[*]} ..."
apt -V install -y "${pkgs[@]}"

echo "[6/6] Optionales DRM Modeset setzen ..."
cat >/etc/modprobe.d/nvidia-drm.conf <<'EON'
options nvidia-drm modeset=1 fbdev=1
EON
update-initramfs -u -k all

echo
echo "Fertig. System neu starten:"
echo "  sudo reboot"
echo
echo "Nach dem Neustart prüfen:"
echo "  nvidia-smi"
echo "  lsmod | grep nvidia"
