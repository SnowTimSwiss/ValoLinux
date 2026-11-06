# Zu verifizieren

Ergebnisse hier eintragen (Datum, Methode, Resultat). Bis dahin gilt alles als **ungeprüft**.

| Punkt | Status | Ergebnis |
|---|---|---|
| ArtCraft-Suite: Reifegrad, DOCX/XLSX/PPTX-Verlust | offen | Pre-Alpha/Alpha laut Berichten |
| FlameWM | nicht mehr verwendet (ab 0.0.4 XFCE) | |
| XFCE-Einrichtung (Panel, Whisker, xfwm4-Theme, Tastenkürzel) | in Vorschau geprüft 2026-11-05 (Ubuntu 24.04, XFCE 4.18, Xvfb) | Im ISO ist XFCE 4.20 (trixie): Panel-Positionswerte, Plugin-Eigenschaften und `dm-tool lock` auf echtem Gerät prüfen |
| iroh 1.0: eigener Relay konfigurierbar? | offen | |
| Firefox ESR `policies.json`: Extensions erzwingen, Einstellungen sperren; welcher Pfad gilt bei Debians `firefox-esr` (`/etc/firefox/policies/` oder `/usr/lib/firefox-esr/distribution/`)? | offen | Agent schreibt vorerst beide; im ISO prüfen mit `about:policies` |
| Ecosia Family-Friendly-Modus per Policy/Suchmaschinen-URL setzbar? | offen | |
| Adblocker (z. B. uBlock Origin) per Policy erzwingen | offen | |
| RAM/Image-Grössen (Debian minimal, Desktop, Firefox) | offen | per `tools/phase0-measure.sh` messen |
| Debian-Backports-Kernel für N100, Release-Zyklus | offen | |
| RAUC vs. systemd-sysupdate | offen | |
| bootc auf Debian als Alternative zum Eigenbau | offen | |
| Flatpak: Speicher/RAM der Runtimes, eigener signierter Remote, Offline-Verhalten | offen | |
| Roaming-Home (Nextcloud-Mount vs. Sync) Praxistest | offen | siehe home-und-konten.md |
| Firefox-Policies `ManagedBookmarks`, `WebsiteFilter` (Sperr-/Positivliste), `ExtensionSettings` `*`=blocked: Wirkung in Firefox ESR prüfen; blockiert die Positivliste die erzwungene uBlock-Installation? | offen | Agent erzeugt sie, siehe `agent/src/apply.rs` |
| Ecosia-Suchparameter / Family-Friendly | offen | derzeit nur Standard-Suche |
| Agent auf echtem Debian trixie (Container `debian:trixie-slim`) | teilweise geprüft 2026-11-05 | Tastatur, Zeitzone, Firefox-Richtlinien, polkit-Regel, Hintergrund, `valoadmin` anlegen, Passwort-Hash (mit `crypt` geprüft), Hostname: ok. `locale-gen` und `flatpak` fehlten im Container (im ISO enthalten), nicht geprüft. Gefundener Fehler: Passwort-Hash mit zu langem Salt (behoben, alte Hashes werden beim Serverstart neu berechnet). |
| Logout-Skript `session-cleanup` (Home löschen) | im Container geprüft 2026-11-05 | löscht nur `/home/valo` bei Modus `wipe`, stellt aus `/etc/skel` wieder her. **Offen:** setzt LightDM beim `session-cleanup-script` wirklich `USER`? Auf echtem Gerät prüfen. |
| live-build 1:20250505 (trixie): Bootloader-Vorlagen | geprüft am Quellcode 2026-11-05 | Vorlagen enthalten kein „Debian“, Splash ist `splash.svg` (mit `rsvg-convert` gerendert), eigene Dateien in `config/bootloaders/` überlagern die Vorlagen. Titel: `syslinux_common/menu.cfg` (`menu title Boot menu` + Piepton-Zeichen), `grub-pc/live-theme/theme.txt` (`title-text`). Quelle: Spiegel github.com/Kicksecure/live-build. |

## ISO: Boot-Fehler „invalid magic number" mit Ventoy (0.0.4)

- **Gemessen (Testbuild `dev-6`, Log):** Squashfs 1,5 GB (zstd), `/live/vmlinuz` bei Sektor 867.772 (≈ 1,78 GB), `/live/initrd.img` bei 809.694, Installer-Kernel `/install/gtk/vmlinuz` bei 41.192. Alle Boot-Dateien liegen **unter** 2 GiB (Sektor 1.048.576), der Kernel hat gültigen Header (`HdrS`), die `grub.cfg` zeigt auf die richtigen Pfade. Die Theorie „Kernel hinter 2 GiB" ist damit **widerlegt**.
- Ein Versuch, Kernel/initrd per `--sort-weight` nach vorn zu legen, hat im Build nicht gegriffen und wurde wieder entfernt (lokal mit xorriso 1.5.6 funktionierte `--sort-weight N /iso/pfad` nach dem Quellpfad).
- Ursache bleibt **offen**. Verbleibende Unterschiede zu 0.0.3: eigene `grub.cfg` (deutsche Texte mit Umlauten/„…"), ISO ≈ 2,2 GB, initrd 118 MB (Firmware).
