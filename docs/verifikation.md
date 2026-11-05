# Zu verifizieren

Ergebnisse hier eintragen (Datum, Methode, Resultat). Bis dahin gilt alles als **ungeprüft**.

| Punkt | Status | Ergebnis |
|---|---|---|
| ArtCraft-Suite: Reifegrad, DOCX/XLSX/PPTX-Verlust | offen | Pre-Alpha/Alpha laut Berichten |
| FlameWM: echter RAM-Bedarf (README ~31+44 MB vs. Titel ~17 MB), X11-Stabilität | offen | |
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
