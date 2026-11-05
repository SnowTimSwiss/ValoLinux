# Was ist im Image (Stand: Test-ISO, ungetestet)

| Bereich | Inhalt |
|---|---|
| Desktop | XFCE, eingerichtet ähnlich Windows 11: helle Taskleiste unten mit Startknopf (Whisker-Menü) und angehefteten Apps in der Mitte, Uhr/Systembereich rechts, eigenes Fensterdesign „ValoLinux“ (flache Knöpfe _ □ ✕), Arc-GTK-Theme, Papirus-Symbole. LightDM mit eigenem Greeter, Autologin als `valo` |
| Office | WordCraft, GridCraft, DeckCraft (ArtCraft, Pre-Alpha, aus GitHub-Releases), im Menü als „Textverarbeitung“, „Tabellen“, „Präsentation“ (Suchwörter: Word, Excel, PowerPoint) |
| Browser | Firefox ESR, uBlock Origin per Policy, Updates aus |
| Software | Flatpak installiert, noch kein eigener Remote/Katalog |
| System | zram (RAM/2, zstd), NetworkManager, Firmware (`firmware-linux`), Mesa-Treiber |
| Dazu | Dateien (Thunar), Editor (Mousepad), Bildbetrachter (Ristretto), Task-Manager, Bildschirmfoto, Terminal, Lautstärke (PipeWire), Netzwerk (nm-applet) |
| Tastenkürzel | Win = Startmenü, Win+L = sperren (unser Greeter), Win+E = Dateien, Win+D = Desktop, Win+Pfeile = Fenster anordnen, Strg+Alt+Entf / Strg+Shift+Esc = Task-Manager, Druck / Win+Shift+S = Bildschirmfoto |
| Nicht drin | LibreOffice (Fallback), Ecosia/Family-Friendly, Drucker (CUPS), Nextcloud-Client, Agent |

Branding: siehe `docs/design.md` (Logo, Farben, Wallpaper, Greeter-Optionen).
Boot-Menü: Titel „ValoLinux“, Einträge Live und Installer (`--debian-installer live`) – Umbenennung der Einträge per `sed` ist best effort und **ungetestet**.
