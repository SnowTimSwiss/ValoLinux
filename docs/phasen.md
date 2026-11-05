# Phasen

- [~] **Phase 0 – Messen**: übersprungen (Entscheidung 2026-11-04)
  - ursprünglich: Testgerät, Debian minimal + XFCE/LXQt + Firefox, `tools/phase0-measure.sh`, Ergebnisse in `docs/verifikation.md`.
- [ ] **Phase 1 – Bootfähiges Image** (in Arbeit, Entwurf der Live-ISO via live-build in `image/`, ungetestet): mkosi/live-build, QEMU, Kiosk-User, Firefox-Policies, zram.
- [ ] **Phase 2 – Updates**: A/B + Rollback, signiertes Image via GitHub Actions, Rollback-Test mit kaputtem Image.
- [~] **Phase 3 – Verwaltung (MVP)**: statt ansible-pull direkt eigener Rust-Agent. Erledigt: Web-UI (Adoption per Code, Rollen, Gruppen, Einstellungen mit Vererbung, App-Katalog), signierte Manifeste, Admin-Passwort pro Gerät, Docker. Offen: 2FA, Konten/Benutzer, Test auf echtem Gerät.
- [ ] **Phase 4 – Transport**: Rust-Agent mit iroh, eigener Relay, signierte Manifeste, Katalog-only.
- [ ] **Phase 5 – Pilot**: eine Klasse, 2–3 Lehrpersonen, gestaffelte Freigabe (Testgerät → Klasse → alle).

## Anmerkungen
- Builds laufen über GitHub Actions **nur bei einem Release** (`.github/workflows/build-image.yml`, Entwurf, ungetestet); die ISO wird ans Release gehängt. Lokal ist nichts nötig; zum Testen lädt man das Artefakt und bootet es in QEMU.
- Ein installierbares Medium (.iso/.img zum Flashen) und das Update-Image (A/B, Phase 2) sind zwei Ausgabeformate derselben Build-Pipeline. Welches Format zuerst, ist offen.
- Server-Sprache: siehe `docs/entscheidungen.md`.
