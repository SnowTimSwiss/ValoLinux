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

# Boot menu branding. live-build (1:20250505) overlays config/bootloaders/* over its
# templates and renders config/bootloaders/splash.svg with rsvg-convert.
# Must not fail the build: branding is cosmetic.
LB_SHARE=/usr/share/live/build/bootloaders
rm -rf config/bootloaders
mkdir -p config/bootloaders/syslinux_common config/bootloaders/grub-pc/live-theme
if [ -f "$LB_SHARE/syslinux_common/menu.cfg" ]; then
  sed 's/^menu title Boot menu/menu title ValoLinux/' "$LB_SHARE/syslinux_common/menu.cfg" \
    > config/bootloaders/syslinux_common/menu.cfg || true
fi
if [ -f "$LB_SHARE/grub-pc/live-theme/theme.txt" ]; then
  sed 's/^title-text:.*/title-text: "ValoLinux"/' "$LB_SHARE/grub-pc/live-theme/theme.txt" \
    > config/bootloaders/grub-pc/live-theme/theme.txt || true
fi
# German labels. GRUB (UEFI): own grub.cfg; syslinux (BIOS): relabel the templates.
cp bootloader/grub.cfg config/bootloaders/grub-pc/grub.cfg || true
for f in live.cfg.in install_gui.cfg install_text.cfg utilities.cfg menu.cfg; do
  [ -f "$LB_SHARE/syslinux_common/$f" ] || continue
  [ -f "config/bootloaders/syslinux_common/$f" ] || cp "$LB_SHARE/syslinux_common/$f" config/bootloaders/syslinux_common/
  sed -i \
    -e 's/menu label ^Live system (@FLAVOUR@)$/menu label ValoLinux ^starten/' \
    -e 's/menu label Live system (@FLAVOUR@ fail-safe mode)$/menu label ValoLinux starten (^abgesicherter Modus)/' \
    -e 's/menu label Start ^installer$/menu label ValoLinux ^installieren/' \
    -e 's/menu label Start installer with ^speech synthesis$/menu label Installieren mit ^Sprachausgabe/' \
    -e 's/menu label ^Advanced install options$/menu label ^Weitere Installationsoptionen/' \
    -e 's/menu title Advanced install options$/menu title Weitere Installationsoptionen/' \
    -e 's/menu label ^Utilities$/menu label ^Werkzeuge/' \
    -e 's/menu title Utilities$/menu title Werkzeuge/' \
    -e 's/menu label ^Back\.\.$/menu label ^Zurück/' \
    "config/bootloaders/syslinux_common/$f" || true
done

SPLASH=config/includes.chroot/usr/share/backgrounds/valolinux-splash.png
if [ -f "$SPLASH" ]; then
  {
    printf '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="640" height="480">'
    printf '<image width="640" height="480" xlink:href="data:image/png;base64,'
    base64 -w0 "$SPLASH"
    printf '"/></svg>\n'
  } > config/bootloaders/splash.svg
fi
find config/bootloaders -type d -empty -delete || true

lb build
find . -maxdepth 1 -name 'valolinux*.iso' -exec mv {} "$out/" \;
ls -lh "$out"
