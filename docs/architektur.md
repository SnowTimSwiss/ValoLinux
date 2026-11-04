# Architektur

## Komponenten
1. **Image-Builder** (`image/`): `mkosi` oder `live-build`, `--no-install-recommends`, Firmware gekürzt (N100, Ryzen). zram aktiv.
2. **Update-System**: A/B-Partitionen + Rollback (RAUC vs. `systemd-sysupdate` vergleichen), read-only Root, Signaturprüfung (Cosign/minisign), öffentlicher Schlüssel im Image.
3. **Zustandslos (Standard)**: Home wird beim Abmelden gelöscht, **einstellbar in der Web-UI**, siehe `home-und-konten.md`; Dateien in der Cloud (z. B. Nextcloud). Defektes Gerät = neu flashen + neu adoptieren.
4. **Agent**: holt Manifest, wendet idempotent an, cached offline. Prototyp: `ansible-pull` + Git. Später Rust-Agent.
5. **Server** (`server/`): Web-UI + API in Docker; Geräte adoptieren, Rolle/Gruppe, Manifest pflegen.
6. **Adoption**: Gerät zeigt Kurzcode → Admin bestätigt in Web-UI und weist dort die Rolle zu → Gerät erhält eigene Schlüsselidentität.
7. **Transport**: Agent ↔ Server per iroh; Adoptionscode ist der erste Schlüsselaustausch (iroh liefert ihn nicht).
8. **Software-Katalog**: Manifest darf nur Einträge aus eigenem, signiertem Katalog installieren. **Standard-Backend: Flatpak** (eigener Remote).
9. **Optional/offen**: Kopplung mit Moniqo.

## Rollen
- **Schüler**: gesperrt (dconf/polkit), keine Adminrechte.
- **Lehrer**: mehr Rechte; **welche genau, stellt der Admin in der Web-UI ein** (Rechte-Profil pro Rolle im Manifest, nicht hartcodiert).
- **Admin**: ein zufälliges Passwort pro Gerät (wie LAPS), in der Web-UI abrufbar.

## Sicherheit
- Wer den Server kontrolliert, hat Root auf allen Geräten → Manifeste und Images signieren, Web-UI nicht offen im Internet, 2FA.
- Firefox: `policies.json` erzwingen/sperren (vor Einsatz verifizieren).
- iroh: eigener Relay statt n0-Relays für Schuldaten (Konfigurierbarkeit prüfen).
