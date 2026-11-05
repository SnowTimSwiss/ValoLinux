# Test-Checkliste Live-ISO

Zum Abhaken beim Test zuhause. Bei Problemen: Foto/Screenshot + was passiert ist.

**Build (GitHub Actions)**
- [ ] Workflow grün? Falls rot: Fehlerzeilen aus dem Log kopieren (die Paket-Vorprüfung meldet fehlende Pakete gleich am Anfang).

**Boot**
- [ ] Boot-Menü zeigt „ValoLinux“ und unseren Splash? Gibt es Live- und Installer-Einträge?
- [ ] Bootet bis zur grafischen Oberfläche?

**Anmeldung**
- [ ] Autologin direkt in FlameWM?
- [ ] Falls Anmeldemaske: `valo` / `valo` probieren. Sieht der Greeter wie in `docs/design.md` aus?
- [ ] Falls FlameWM nicht startet: im Greeter oben die Sitzung „Openbox“ wählen und anmelden, Rechtsklick auf den Desktop öffnet ein Menü.
- [ ] `Ctrl+Alt+F3`: Diagnose-Block über dem Login abfotografieren. Login dort mit `root` / `valo`.

**Desktop**
- [ ] Hintergrundbild da?
- [ ] Tastatur Schweizerdeutsch (z/y, Umlaute)? Uhrzeit korrekt (Europe/Zurich)?
- [ ] Firefox startet, uBlock Origin ist installiert?
- [ ] WordCraft, GridCraft, DeckCraft starten? Eine DOCX/XLSX/PPTX öffnen.
- [ ] Netzwerk (LAN/WLAN) geht?

**Wenn Zeit bleibt**
- [ ] `free -m` direkt nach dem Start und mit Firefox + WordCraft offen (Terminal: FlameWM oder xterm).
- [ ] Installer im Boot-Menü ausprobieren (nur in einer VM!).
