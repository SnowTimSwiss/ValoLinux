# image/ – Live-ISO (Test-Image)

Ziel: per GitHub Actions bei einem Release eine bootbare ISO bauen, mit **XFCE** (wie Windows eingerichtet), **WordCraft/GridCraft/DeckCraft**, Firefox ESR, zram. Zum Anschauen, wie es aussieht (QEMU/VirtualBox/USB). **Noch kein A/B-Update-Image** (Phase 2).

**Status: ungetestet.** Das Authoring-Umfeld hatte weder live-build noch QEMU. Der erste CI-Lauf wird vermutlich Fehler zeigen; die Log-Ausgabe bitte zurückmelden.

- `build.sh`: ruft `fetch-apps.sh` auf, konfiguriert und baut mit `live-build` (Debian trixie, hybrid ISO).
- `fetch-apps.sh`: lädt die `.deb`s der Apps aus den neuesten GitHub-Releases und benennt sie für live-build um (`name_version_amd64.deb`).
- `config/package-lists/`: Debian-Pakete.
- `config/includes.chroot/`: Dateien im Image (zram, LightDM-Autologin in die Sitzung „ValoLinux“, XFCE-Vorgaben in `/etc/xdg/xfce4`, Firefox-Policies, Startmenü-Namen der Apps).

Test-Image: Anmeldung `valo`/`valo` und `root`/`valo` (nur zum Debuggen, vor echtem Einsatz entfernen). `valo-diag` schreibt den Login-Zustand beim Boot in `/etc/issue` (sichtbar vor dem Konsolen-Login, Ctrl+Alt+F3).

Nicht verifiziert: Pfad/Wirkung von `policies.json`, Ecosia-Standardsuche (bewusst noch weggelassen, siehe `docs/verifikation.md`), Benutzer `valo` (Passwort `valo`, per Hook angelegt). Das Live-System hat Standard-Rechte von Debian Live (sudo) und ist **kein** gesperrter Kiosk. Das kommt später.
