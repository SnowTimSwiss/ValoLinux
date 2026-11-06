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

# VALO_FAST=1: quick test build (zstd squashfs is much faster to compress than xz)
fast=()
[ -z "${VALO_FAST:-}" ] || fast=(--chroot-squashfs-compression-type zstd)

lb clean --purge || true
lb config \
  "${fast[@]}" \
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

# Boot files first: live-build's xorriso call lays files out in name order, so the
# kernel/initrd end up behind the (multi-GB) squashfs, beyond 2 GiB. Some loaders
# (GRUB via Ventoy, old BIOSes) cannot read that far ("invalid magic number").
# Wrapper that adds --sort-weight for them; the weights must come after the source path.
wrapdir="$PWD/.wrapbin"
mkdir -p "$wrapdir"
real_xorriso="$(command -v xorriso)"
cat > "$wrapdir/xorriso" <<WRAP
#!/bin/sh
if [ "\$1" = "-as" ] && [ "\$2" = "mkisofs" ]; then
  extra=""
  for f in binary/live/vmlinuz-* binary/live/initrd.img-* binary/install/vmlinuz binary/install/initrd.gz binary/install/gtk/vmlinuz binary/install/gtk/initrd.gz; do
    [ -e "\$f" ] && extra="\$extra --sort-weight 100 /\${f#binary/}"
  done
  exec "$real_xorriso" "\$@" \$extra
fi
exec "$real_xorriso" "\$@"
WRAP
chmod +x "$wrapdir/xorriso"
export PATH="$wrapdir:$PATH"

lb build

# Where do the boot files sit in the finished ISO? (LBA * 2048 must stay below 2 GiB = LBA 1048576)
iso_file="$(ls valolinux*.iso | head -1)"
echo "=== boot file positions in $iso_file (LBA) ==="
xorriso -indev "$iso_file" -find / -type f \( -name 'vmlinuz*' -o -name 'initrd*' -o -name '*.squashfs' \) -exec report_lba 2>&1 | grep "File data" || true
for k in binary/live/vmlinuz-*; do
  # a bzImage has "HdrS" at byte offset 514
  [ "$(dd if="$k" bs=1 skip=514 count=4 2>/dev/null)" = "HdrS" ] && echo "kernel ok: $k" || { echo "ERROR: $k is not a Linux kernel image"; exit 1; }
done

# Size report: biggest installed packages and the final image parts
echo "=== largest packages (installed size, KiB) ==="
dpkg-query --admindir=chroot/var/lib/dpkg -W -f='${Installed-Size}\t${Package}\n' 2>/dev/null | sort -rn | head -40 || true
ls -lh binary/live/*.squashfs 2>/dev/null || true
du -sh chroot/opt chroot/usr/lib chroot/usr/share chroot/lib/firmware 2>/dev/null || true

# Boot sanity check: show the boot config and make sure every kernel/initrd it
# references exists on the medium (a wrong path gives "invalid magic number /
# you need to load the kernel first" at boot).
echo "=== binary/live ==="; ls -l binary/live 2>/dev/null || true
echo "=== binary/boot/grub/grub.cfg ==="; cat binary/boot/grub/grub.cfg 2>/dev/null || true
bad=0
for f in binary/boot/grub/grub.cfg binary/isolinux/*.cfg; do
  [ -f "$f" ] || continue
  for p in $(grep -hE '^[[:space:]]*(linux|initrd|kernel|append[[:space:]]+initrd=)' "$f" | grep -oE '/(live|install)[^ ]*' | sort -u); do
    case "$p" in *@*|*'$'*) echo "ERROR: unresolved placeholder in $f: $p"; bad=1; continue;; esac
    [ -e "binary$p" ] || { echo "ERROR: $f refers to missing $p"; bad=1; }
  done
done
[ "$bad" = 0 ] || { echo "ERROR: boot config is inconsistent"; exit 1; }
find . -maxdepth 1 -name 'valolinux*.iso' -exec mv {} "$out/" \;
ls -lh "$out"
