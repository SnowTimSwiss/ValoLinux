# image/ – Live-ISO (Test-Image)

Ziel: per GitHub Actions bei einem Release eine bootbare ISO bauen, mit **FlameWM**, **WordCraft/GridCraft/DeckCraft**, Firefox ESR, zram. Zum Anschauen, wie es aussieht (QEMU/VirtualBox/USB). **Noch kein A/B-Update-Image** (Phase 2).

**Status: ungetestet.** Das Authoring-Umfeld hatte weder live-build noch QEMU. Der erste CI-Lauf wird vermutlich Fehler zeigen; die Log-Ausgabe bitte zurückmelden.

- `build.sh`: ruft `fetch-apps.sh` auf, konfiguriert und baut mit `live-build` (Debian trixie, hybrid ISO).
- `fetch-apps.sh`: lädt die `.deb`s der Apps aus den neuesten GitHub-Releases (WordCraft laut README; GridCraft/DeckCraft-Assetnamen **angenommen**). FlameWM: Release-`.deb`, sonst Build aus dem Quellcode.
- `config/package-lists/`: Debian-Pakete.
- `config/includes.chroot/`: Dateien im Image (zram, LightDM-Autologin in FlameWM, Firefox-Policies).

Nicht verifiziert: Pfad/Wirkung von `policies.json`, Session-Name `flamewm` für LightDM, Ecosia-Standardsuche (bewusst noch weggelassen, siehe `docs/verifikation.md`), Benutzer `valo` (Passwort `valo`, per Hook angelegt). Das Live-System hat Standard-Rechte von Debian Live (sudo) und ist **kein** gesperrter Kiosk. Das kommt später.
