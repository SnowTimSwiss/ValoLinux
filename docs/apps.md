# Was ist im Image (Stand: Test-ISO, ungetestet)

| Bereich | Inhalt |
|---|---|
| Desktop | FlameWM (X11-Sitzung), LightDM mit GTK-Greeter, Autologin als `valo` |
| Office | WordCraft, GridCraft, DeckCraft (ArtCraft, Pre-Alpha, aus GitHub-Releases) |
| Browser | Firefox ESR, uBlock Origin per Policy, Updates aus |
| Software | Flatpak installiert, noch kein eigener Remote/Katalog |
| System | zram (RAM/2, zstd), NetworkManager, Firmware (`firmware-linux`), Mesa-Treiber |
| Dazu von FlameWM | Explorer, Terminal, Screenshot, Task-Manager, Archiver, Bildbetrachter u. a. (laut FlameWM-Paket) |
| Nicht drin | LibreOffice (Fallback), Ecosia/Family-Friendly, Drucker (CUPS), Nextcloud-Client, Agent |

Branding: siehe `docs/design.md` (Logo, Farben, Wallpaper, Greeter-Optionen).
Boot-Menü: Titel „ValoLinux“, Einträge Live und Installer (`--debian-installer live`) – Umbenennung der Einträge per `sed` ist best effort und **ungetestet**.
