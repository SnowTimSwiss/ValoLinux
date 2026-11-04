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
  --debian-installer none \
  --apt-recommends false \
  --security true --updates true \
  --iso-volume "VALOLINUX" \
  --image-name valolinux \
  --bootappend-live "boot=live components quiet splash"
lb build
find . -maxdepth 1 -name 'valolinux*.iso' -exec mv {} "$out/" \;
ls -lh "$out"
