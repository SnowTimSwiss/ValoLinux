#!/usr/bin/env bash
# Fetch app .debs into live-build's packages.chroot (installed in the image incl. dependencies).
# DRAFT, UNTESTED. Release asset names are taken from the upstream READMEs, not verified:
#   WordCraft: wordcraft-<ver>-linux-x86_64.deb (documented in its README)
#   GridCraft/DeckCraft: assumed to follow the same pattern -> the script fails loudly if not.
# Needs: gh (with GH_TOKEN) and dpkg-deb.
set -euo pipefail
dest="${1:?usage: fetch-apps.sh <dest-dir>}"
mkdir -p "$dest"
dest="$(cd "$dest" && pwd)"

for app in wordcraft gridcraft deckcraft; do
  echo "== $app"
  gh release download --repo "storytold/$app" --pattern '*-linux-x86_64.deb' --dir "$dest" --clobber \
    || { echo "ERROR: no x86_64 .deb found in latest release of storytold/$app"; exit 1; }
done

# live-build only picks up *_<arch>.deb / *_all.deb from packages.chroot, so rename
# e.g. wordcraft-0.15.2-linux-x86_64.deb -> wordcraft_0.15.2_amd64.deb
for f in "$dest"/*.deb; do
  pkg=$(dpkg-deb --field "$f" Package)
  ver=$(dpkg-deb --field "$f" Version)
  arch=$(dpkg-deb --field "$f" Architecture)
  want="$dest/${pkg}_${ver}_${arch}.deb"
  [ "$f" = "$want" ] || mv -f "$f" "$want"
done
ls -lh "$dest"
