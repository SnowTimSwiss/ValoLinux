# Home-Verhalten und zentrale Konten

Stand: 2026-11-04 – **Entwurf, noch nicht entschieden.**

## Anforderung
1. Ob `/home` beim Abmelden gelöscht wird, ist in der Web-UI einstellbar (Rolle, Gruppe oder einzelnes Gerät; Standard: löschen).
2. Idee: ein zentraler Account pro Person, der auf jedem Gerät gleich funktioniert, inklusive Dateien (im Prinzip das ganze `/home`).

## Manifest-Optionen (Vorschlag)
```yaml
home:
  mode: wipe        # wipe | persist-local | roaming
  roaming_paths: [Documents, Desktop, Downloads]   # nur bei roaming
  quota_mb: 2048
```

## Varianten für zentrale Dateien
| Variante | Prinzip | Vorteile | Nachteile |
|---|---|---|---|
| A: Nextcloud/WebDAV | Dateien per Mount (rclone/davfs) oder Sync-Client | Passt zur Cloud-Idee, kein eigenes Protokoll | Latenz, Offline schwach |
| B: Netzwerk-Home (NFS/SMB) mit pam_mount | Home liegt auf dem Server | Echt „das ganze Home" | Braucht zentrales Verzeichnis (LDAP/Kerberos), Schule-LAN zwingend, mehr Betrieb |
| C: Sync bei Login/Logout (rsync/restic-artig) | Ausgewählte Ordner werden geholt/zurückgeschrieben | Lokal schnell, Offline nach Login | Konflikte bei zwei Geräten gleichzeitig, Logout muss sauber enden |

## Empfehlung
**Nicht das ganze `/home` roamen**, sondern definierte Ordner (A oder C). Gründe:
- Dotfiles/Caches (v. a. Firefox-Profil) sind gross, ändern sich ständig und kollidieren zwischen Geräten. Firefox wird ohnehin per `policies.json` gesteuert, ein Profil muss nicht wandern.
- Ganzes Home widerspricht dem zustandslosen Konzept (Gerät neu flashen = nichts geht verloren) nicht, macht aber Login langsam und Fehlerfälle komplex.
- Mit `roaming_paths` und Quota bleibt es einstellbar und Admin-kontrolliert.

Startpunkt für den Test: **Variante A** (Nextcloud, ist laut Briefing ohnehin geplant), Variante C nur falls Offline-Betrieb nötig wird.

## Konten
- Konten (Schüler/Lehrer) werden in der Web-UI geführt, Rolle hängt am Konto **und** am Gerät (Gerätrolle begrenzt, was möglich ist).
- Anmeldung am Gerät gegen den Server, mit lokalem Cache für Offline-Login. Mögliche Anbindung an bestehendes Verzeichnis (Nextcloud/OIDC/LDAP) später.
- Das ist ein neuer Bestandteil: Konto-Verwaltung und PAM/Login-Integration (z. B. über OIDC oder `sssd`) – Aufwand in Phase 3/4 einplanen, nicht in Phase 1.

## Was der Admin in der Web-UI einstellen kann (Entwurf)
Alle Einstellungen gelten pro **Rolle**, **Gruppe/Klasse** oder **Gerät**; spezifischer überschreibt allgemeiner.

| Einstellung | Werte |
|---|---|
| Home-Modus | `wipe` / `persist-local` / `roaming` |
| Roaming-Ordner | Liste (z. B. Documents, Desktop) |
| Quota pro Konto | MB |
| Anmeldeart am Gerät | Gemeinsames Gerätekonto (ohne Login) / persönliches Konto |
| Offline-Login erlaubt, Cache-Dauer | ja/nein, Tage |
| Kontoquelle | Web-UI-intern / später OIDC, LDAP, Nextcloud |
| Konten anlegen | Einzeln, CSV-Import pro Klasse |
| Passwort zurücksetzen | Lehrer für eigene Klasse (Recht einstellbar) |
| Lehrer-Zugriff auf Schülerdateien | aus / nur eigene Klasse / alle |
| Aufbewahrung/Löschung beim Klassenwechsel/Jahresende | Frist, automatisch löschen oder archivieren |
| Gerätesperre ausserhalb Unterricht | optional, Zeitplan |

Wichtig: Der Gerätezustand bleibt über das Manifest steuerbar, Konten und Dateien sind davon getrennt.

## Vorschlag für die Umsetzung (Stand 2026-11-05, noch nicht gebaut)
Die Verwaltung (Server, Agent, signierte Manifeste) steht. Darauf aufbauend gibt es zwei realistische Wege:

| | A: Konten in der Web-UI, auf Geräte synchronisiert | B: Anbindung an bestehende Schulkonten (OIDC/LDAP, z. B. Microsoft 365, Google) |
|---|---|---|
| Wie | Admin legt Konten an (einzeln/CSV pro Klasse). Server schickt jedem Gerät die Konten, die dort erlaubt sind (z. B. seine Klasse) mit Passwort-Hash. Agent legt lokale Benutzer an. | Gerät prüft das Passwort bei der Schul-Anmeldung (z. B. `sssd`, OIDC-PAM-Modul). |
| Offline | funktioniert (Hashes liegen lokal) | nur mit Cache |
| Aufwand | klein, alles schon da (Manifest, Agent, UI) | gross, abhängig vom Anbieter |
| Sicherheit | Passwort-Hashes liegen auf jedem Gerät der Klasse (nur für root lesbar). Schwache Kinderpasswörter sind knackbar, wenn jemand Root auf einem Gerät hat. | keine Hashes auf Geräten |
| Passwort zurücksetzen | in der Web-UI, auch durch Lehrperson (Recht einstellbar) | beim Anbieter |

**Empfehlung:** A als Start, aber nur Konten der eigenen Klasse/Gruppe pro Gerät, Mindestlänge, und Dateien nicht lokal sondern in der Cloud (Nextcloud). B später, falls die Schule schon zentrale Konten hat.

## Offene Fragen an dich
1. Gibt es schon einen Nextcloud-Server/Verzeichnisdienst in der Schule, oder soll die Web-UI die Konten selbst führen?
2. Datenschutz: Schülerdateien liegen dann zentral – wo wird gehostet (Schulhaus, Gemeinde, extern) und wer darf zustimmen? (Rechtlich nicht von mir geprüft.)
3. Sollen Lehrer auf Dateien ihrer Klasse zugreifen können?
4. Müssen Geräte ohne Netz nutzbar sein?
