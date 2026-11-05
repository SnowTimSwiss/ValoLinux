# ValoLinux

Minimales, zentral verwaltetes Linux-Image für Schulgeräte (Kiosk-Konzept).
Langfristiges Einzelprojekt, aktuell **Testprojekt – kein Produktivbetrieb vor dem Pilot (Phase 5)**.

ValoLinux ist **keine eigene Distribution**, sondern ein Image-Builder (auf Debian Stable)
plus eine Verwaltungsschicht (Agent + Web-UI).

## Idee in Kürze
- Image flashen → Gerät in der Web-UI **adoptieren** → Rolle (Lehrer/Schüler) zuweisen → fertig.
- Geräte holen Konfiguration per Pull vom Server (Manifest = Soll-Zustand).
- Updates image-basiert (A/B, signiert, Rollback), keine Paket-Updates auf dem Gerät.
- Zustandslos: Home wird beim Abmelden gelöscht.

## Struktur
| Pfad | Inhalt |
|---|---|
| `docs/entscheidungen.md` | Getroffene Entscheidungen und Begründungen |
| `docs/architektur.md` | Architektur, Rollen, Sicherheit |
| `docs/home-und-konten.md` | Home-Verhalten, zentrale Konten (Entwurf) |
| `docs/phasen.md` | Phasenplan mit Fortschritt |
| `docs/verifikation.md` | Unsichere Fakten: was zu prüfen ist, Ergebnisse |
| `docs/verwaltung.md` | Server, Web-UI, Agent: Start, Adoption, Einstellungen |
| `image/` | Live-ISO (live-build), Build per GitHub Actions bei Release |
| `crates/valo-proto/` | Gemeinsame Typen und Signaturen (Rust) |
| `agent/` | Geräte-Agent (Rust) |
| `server/` | Web-UI + API (Rust, Docker) |
| `manifests/` | Beispiel-Manifeste je Rolle |
| `tools/` | Hilfsskripte (z. B. RAM-Messung Phase 0) |

## Stand (2026-11-05)
| Teil | Zustand |
|---|---|
| Web-UI + Server (Rust, Docker) | funktioniert, automatisch getestet (Rust-Tests + Browser), Docker-Image lokal gebaut |
| Agent (Rust) | funktioniert gegen den Server, Anwenden nur im Testverzeichnis (`--root`) geprüft, **nicht auf echtem Debian** |
| Live-ISO (live-build, GitHub Actions) | **ungetestet**, erster Release-Build steht aus; Login-Problem aus `alpha-v0.0.1` noch offen |
| Updates A/B (Phase 2), iroh (Phase 4), Konten | offen |

Funktionen der Verwaltung: Geräte per Code hinzufügen, Rollen (Schüler/in, Lehrperson), Gruppen/Klassen, Einstellungen mit Vererbung (Tastatur, Sprache, Zeitzone, Home löschen, Browser inkl. Lesezeichen und Webfilter, Apps aus Katalog, Rechte, Hintergrundbild), Neustart/Ausschalten aus der Ferne, Admin-Passwort pro Gerät, Admins mit 2FA.

## Schnellstart Verwaltung
```bash
docker compose -f server/docker-compose.yml up -d --build
```
Dann `http://<server>:8080` öffnen. Details: `docs/verwaltung.md`.

![Übersicht](docs/img/ui/09-dashboard.png)

## Arbeitsregeln
Siehe `docs/entscheidungen.md`, Abschnitt „Regeln". Kurz: bewährte Technik, alles reversibel,
erst in QEMU testen, nichts erfinden (verifizieren und notieren), bei Sicherheit/Schlüsseln nachfragen.
