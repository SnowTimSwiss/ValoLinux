# Phasen

- [ ] **Phase 0 – Messen**: Testgerät, Debian minimal + XFCE/LXQt + Firefox, `tools/phase0-measure.sh`, Ergebnisse in `docs/verifikation.md`.
- [ ] **Phase 1 – Bootfähiges Image**: mkosi/live-build, QEMU, Kiosk-User, Firefox-Policies, zram.
- [ ] **Phase 2 – Updates**: A/B + Rollback, signiertes Image via GitHub Actions, Rollback-Test mit kaputtem Image.
- [ ] **Phase 3 – Verwaltung (MVP)**: ansible-pull + Web-UI (Kurzcode, Rolle, Gruppe), Admin-Passwort pro Gerät.
- [ ] **Phase 4 – Transport**: Rust-Agent mit iroh, eigener Relay, signierte Manifeste, Katalog-only.
- [ ] **Phase 5 – Pilot**: eine Klasse, 2–3 Lehrpersonen, gestaffelte Freigabe (Testgerät → Klasse → alle).
