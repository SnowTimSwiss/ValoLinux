# Entscheidungen

Stand: 2026-11-04

| Thema | Entscheidung | Begründung |
|---|---|---|
| Basis | Debian Stable, minimales Image | Security-Fixes bleiben bei Debian; wir pflegen nur Image-Builder + Verwaltung |
| Updates | Image-basiert, signiert, A/B mit Rollback | Wenig Wartung, vorhersehbarer Zustand |
| Build | GitHub Actions, Freigabe nach Test auf Testgerät | Einzelentwickler, Automatisierung |
| Verwaltung | Web-UI in Docker, Geräte holen per Pull | Keine eingehenden Verbindungen zu Geräten |
| Transport | iroh (Rust, QUIC) | Ausdrücklich für den Test gewünscht |
| Desktop | FlameWM (X11) | Siehe Verifikation: noch nicht produktionsreif |
| Browser | Firefox ESR, `policies.json`, Adblocker vorinstalliert, Ecosia (Family-Friendly-Modus) als Standardsuche | Zentrale Steuerung |
| Office | ArtCraft-Suite, LibreOffice/OnlyOffice als Fallback im Katalog | ArtCraft ist Pre-Alpha |
| Software-Katalog | **Flatpak als Standard** (eigener, signierter Remote) | Distro-neutral, passt zum Rückweg; Speicher-/RAM-Kosten der Runtimes messen |
| Home-Verhalten | Löschen beim Abmelden ist **in der Web-UI einstellbar** (pro Rolle/Gruppe/Gerät) | Flexibilität, Default: löschen |
| Verworfen | Arch, Fedora bootc (vorerst), NixOS (evtl. später), eigene Distro | Pflegeaufwand |

## Sprachwahl (Entwurf)
- **Agent auf dem Gerät: Rust.** Ein einzelnes statisches Binary, keine Laufzeit im Image, und iroh ist selbst Rust.
- **Server/Web-UI Phase 3: noch offen** (Python/FastAPI schneller, Rust ein Werkzeug für alles). Für den MVP reicht normales HTTPS, iroh wird erst in Phase 4 nötig.

## Rückweg / Distro-Unabhängigkeit
Frage aus dem Briefing: „oder würde etwas anderes Sinn machen?"

Empfehlung: Die Unabhängigkeit liegt in **drei klar getrennten Schichten**, nicht in der Distro-Wahl:
1. **Manifest-Format** (deklarativ, distro-neutral: Rolle, Browser-Policy, Katalog-Einträge, Rechte).
2. **Agent** (Rust, wendet Manifest an, kennt nur ein kleines „Backend"-Interface: Paket/Flatpak installieren, Datei schreiben, Dienst steuern).
3. **Image-Build + Update-Mechanismus** (der einzige distro-spezifische Teil, austauschbar gegen Fedora bootc).

Flatpak als Software-Katalog-Backend wäre distro-neutraler als ein eigenes apt-Repo, kostet aber RAM/Speicher
(Runtimes) – gegen Messung abwägen. Offen: ob bootc auf Debian reif genug ist, wäre zu prüfen (nicht verifiziert).

## Regeln
- Langweilige, bewährte Technik vor Neuem; neue Abhängigkeiten kurz begründen.
- Jede Entscheidung reversibel, v. a. die Basisdistro.
- Alles zuerst in QEMU, dann Testhardware. Keine destruktiven Befehle auf echten Geräten ohne Rückfrage.
- Nichts erfinden: Fakten verifizieren und in `docs/verifikation.md` notieren.
- Bei Unklarheit (Sicherheit, Schlüssel) nachfragen.
- Dokumentation laufend pflegen (Warum, Build, Gerät zurücksetzen).

## Nicht-Ziele
Eigene Distro mit Paket-Updates; Lern-Software; Windows-Layer; Heimnutzung; Einstellungs-App;
Produktivbetrieb vor dem Pilot.
