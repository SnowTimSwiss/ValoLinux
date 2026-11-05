#!/usr/bin/env bash
# Builds the ValoLinux live ISO. Must run as root on Debian (trixie) with live-build installed.
# DRAFT, UNTESTED (authoring environment has no live-build/KVM). CI runs this on release.
set -euo pipefail
cd "$(dirname "$0")"
out="${1:-$PWD/out}"
mkdir -p config/packages.chroot "$out"

./fetch-apps.sh config/packages.chroot

lb clean --purge || true
lb config \
  --distribution trixie \
  --architectures amd64 \
  --archive-areas "main contrib non-free-firmware" \
  --binary-images iso-hybrid \
  --debian-installer live \
  --iso-application "ValoLinux" \
  --iso-publisher "ValoLinux" \
  --apt-recommends false \
  --security true --updates true \
  --iso-volume "VALOLINUX" \
  --image-name valolinux \
  --bootappend-live "boot=live components live-config.username=valo"

# Boot menu branding (best effort, UNTESTED): copy live-build's bootloader templates and rename the title.
if [ -d /usr/share/live/build/bootloaders ]; then
  rm -rf config/bootloaders && cp -r /usr/share/live/build/bootloaders config/bootloaders
  grep -rIl -E 'Debian GNU/Linux|Debian Live|Debian' config/bootloaders 2>/dev/null \
    | xargs -r sed -i -e 's/Debian GNU\/Linux/ValoLinux/g' -e 's/Debian Live/ValoLinux/g' -e 's/Debian/ValoLinux/g'
  find config/bootloaders -name 'splash.png' -exec cp config/includes.chroot/usr/share/backgrounds/valolinux-splash.png {} \;
fi
lb build
find . -maxdepth 1 -name 'valolinux*.iso' -exec mv {} "$out/" \;
ls -lh "$out"
