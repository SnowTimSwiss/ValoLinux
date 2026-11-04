# image/ (Phase 1, Entwurf)

Status: **ungetestet.** Dateien sind Entwürfe, nicht gebaut, nicht gebootet.

- `mkosi.conf`: minimale Debian-Stable-Definition (Optionsnamen gegen die installierte mkosi-Version prüfen).
- `mkosi.extra/etc/systemd/zram-generator.conf`: zram.
- `mkosi.extra/etc/firefox/policies/policies.json`: Firefox-Policies. Pfad (Debian: `/etc/firefox/policies/`?), die uBlock-ID/-URL, und dass `"Default": "Ecosia"` ohne zusätzliche `SearchEngines.Add`-Definition funktioniert, sind **nicht verifiziert**. Der Family-Friendly-Modus von Ecosia ist noch offen (siehe `docs/verifikation.md`).

Nächste Schritte: auf einer Maschine mit mkosi + QEMU bauen (`mkosi build`, `mkosi qemu`), Kiosk-User und Autologin/FlameWM ergänzen (FlameWM erst nach Verifikation).
