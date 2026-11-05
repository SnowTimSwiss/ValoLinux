# Test-Checkliste Live-ISO

Zum Abhaken beim Test zuhause. Bei Problemen: Foto/Screenshot + was passiert ist.

**Build (GitHub Actions)**
- [ ] Workflow grün? Falls rot: Fehlerzeilen aus dem Log kopieren (die Paket-Vorprüfung meldet fehlende Pakete gleich am Anfang).

**Boot**
- [ ] Boot-Menü zeigt „ValoLinux“ und unseren Splash? Gibt es Live- und Installer-Einträge?
- [ ] Bootet bis zur grafischen Oberfläche?

**Anmeldung**
- [ ] Autologin direkt in den XFCE-Desktop (helle Taskleiste unten)?
- [ ] Falls Anmeldemaske: `valo` / `valo` probieren. Sieht der Greeter wie in `docs/design.md` aus?
- [ ] `Ctrl+Alt+F3`: Diagnose-Block über dem Login abfotografieren. Login dort mit `root` / `valo`.

**Desktop**
- [ ] Startmenü (Windows-Taste): Favoriten Firefox, Textverarbeitung, Tabellen, Präsentation, Dateien? Suche nach „word“ findet die Textverarbeitung?
- [ ] Angeheftete Apps in der Taskleiste starten? Win+L sperrt und zeigt unseren Greeter?
- [ ] Hintergrundbild da?
- [ ] Tastatur Schweizerdeutsch (z/y, Umlaute)? Uhrzeit korrekt (Europe/Zurich)?
- [ ] Firefox startet, uBlock Origin ist installiert? In Firefox `about:policies` öffnen: sind die Richtlinien aktiv (Screenshot)?
- [ ] WordCraft, GridCraft, DeckCraft starten? Eine DOCX/XLSX/PPTX öffnen.
- [ ] Netzwerk (LAN/WLAN) geht?

**Wenn Zeit bleibt**
- [ ] `free -m` direkt nach dem Start und mit Firefox + WordCraft offen (Terminal über das Startmenü).
- [ ] Installer im Boot-Menü ausprobieren (nur in einer VM!).

**Verwaltung (optional, braucht einen zweiten Rechner mit Docker)**
- [ ] Auf dem Rechner: `docker compose -f server/docker-compose.yml up -d --build`, dann `http://<rechner>:8080` öffnen, Admin anlegen.
- [ ] ISO booten, im Boot-Menü mit `e` (GRUB) bzw. `Tab` (BIOS) an die Kernel-Zeile anhängen: `valo.server=http://<rechner-ip>:8080`.
      Alternative ohne Boot-Parameter: Rechner heisst `valolinux` (dann findet das Gerät `valolinux.local` selbst).
- [ ] Erscheint auf dem Desktop ein Fenster mit einem Code? In der Web-UI unter „Hinzufügen“ eingeben, Rolle wählen.
- [ ] Nach ca. 1 Minute: Gerät in der Web-UI „online“ und „aktuell“? Fehlermeldungen in der Gerätedetailseite notieren.
- [ ] Einstellung ändern (z. B. Tastatur „Schweiz (Französisch)“, Hintergrundbild), abmelden/anmelden: wirkt es?
- [ ] „Neu starten“ in der Web-UI: startet das Gerät innert ca. 1 Minute neu? (Live-System: danach wieder neuer Code, weil nichts gespeichert wird.)
