#!/usr/bin/env bash
# Fetch app .debs into live-build's packages.chroot (installed in the image incl. dependencies).
# DRAFT, UNTESTED. Release asset names are taken from the upstream READMEs, not verified:
#   WordCraft: wordcraft-<ver>-linux-x86_64.deb (documented in its README)
#   GridCraft/DeckCraft: assumed to follow the same pattern -> the script fails loudly if not.
#   FlameWM: release .deb if present, else built from source (./scripts/build-deb).
# Needs: gh (with GH_TOKEN) ; for the FlameWM source fallback: git, rust toolchain, build deps.
set -euo pipefail
dest="${1:?usage: fetch-apps.sh <dest-dir>}"
mkdir -p "$dest"
dest="$(cd "$dest" && pwd)"

for app in wordcraft gridcraft deckcraft; do
  echo "== $app"
  gh release download --repo "storytold/$app" --pattern '*-linux-x86_64.deb' --dir "$dest" --clobber \
    || { echo "ERROR: no x86_64 .deb found in latest release of storytold/$app"; exit 1; }
done

echo "== flamewm"
if ! gh release download --repo ArkFlame/FlameWM --pattern 'flamewm_*_amd64.deb' --dir "$dest" --clobber 2>/dev/null; then
  echo "No FlameWM release .deb, building from source"
  tmp="$(mktemp -d)"
  tag="$(gh release view --repo ArkFlame/FlameWM --json tagName --jq .tagName 2>/dev/null || echo main)"
  git clone --depth 1 --branch "$tag" https://github.com/ArkFlame/FlameWM "$tmp/flamewm" \
    || git clone --depth 1 https://github.com/ArkFlame/FlameWM "$tmp/flamewm"
  ( cd "$tmp/flamewm" && ./scripts/install-deps-ubuntu.sh && ./scripts/build-deb )
  cp "$tmp"/flamewm/.target/dist/flamewm_*_amd64.deb "$dest/"
fi
ls -lh "$dest"
