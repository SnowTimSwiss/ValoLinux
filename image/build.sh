#!/usr/bin/env bash
# Builds the ValoLinux live ISO. Must run as root on Debian (trixie) with live-build installed.
# DRAFT, UNTESTED (authoring environment has no live-build/KVM). CI runs this on release.
set -euo pipefail
cd "$(dirname "$0")"
out="${1:-$PWD/out}"
mkdir -p config/packages.chroot "$out"

# Preflight: fail fast if a package from our list does not exist in Debian (before the long build)
missing=""
for p in $(grep -v -E '^\s*(#|$)' config/package-lists/*.list.chroot); do
  apt-cache show "$p" >/dev/null 2>&1 || missing="$missing $p"
done
if [ -n "$missing" ]; then echo "ERROR: packages not found in Debian:$missing"; exit 1; fi

./fetch-apps.sh config/packages.chroot

# Device agent (Rust), built from this repository
( cd .. && cargo build --release --locked -p valo-agent )
install -D -m 0755 ../target/release/valo-agent config/includes.chroot/usr/local/bin/valo-agent

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
  --bootappend-live "boot=live components live-config.username=valo live-config.locales=de_CH.UTF-8 live-config.keyboard-layouts=ch live-config.timezone=Europe/Zurich"

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
