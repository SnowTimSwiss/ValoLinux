# Zu verifizieren

Ergebnisse hier eintragen (Datum, Methode, Resultat). Bis dahin gilt alles als **ungeprüft**.

| Punkt | Status | Ergebnis |
|---|---|---|
| ArtCraft-Suite: Reifegrad, DOCX/XLSX/PPTX-Verlust | offen | Pre-Alpha/Alpha laut Berichten |
| FlameWM: echter RAM-Bedarf (README ~31+44 MB vs. Titel ~17 MB), X11-Stabilität | offen | |
| iroh 1.0: eigener Relay konfigurierbar? | offen | |
| Firefox ESR `policies.json`: Extensions erzwingen, Einstellungen sperren | offen | |
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
