//! Apply a manifest to a Debian-based system. Each step is idempotent and
//! independent: a failing step is reported, the others still run.
//!
//! This is the only distro-specific part of the agent. Another base (e.g.
//! Fedora) would get its own version of these functions.

use crate::platform::Platform;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use valo_proto::{HomeMode, Manifest, Settings};

pub const ADMIN_USER: &str = "valoadmin";
/// The kiosk user that students/teachers use.
pub const KIOSK_USER: &str = "valo";

/// What the agent remembers between runs (stored in the state dir).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Applied {
    pub revision: Option<String>,
    pub ok: bool,
    pub admin_hash: Option<String>,
    /// Flatpaks installed by us (we never remove anything we did not install).
    pub managed_apps: Vec<String>,
    /// Ids of one-off actions already handled (most recent last).
    #[serde(default)]
    pub done_actions: Vec<i64>,
}

pub struct Outcome {
    pub ok: bool,
    pub messages: Vec<String>,
}

/// Mark new actions as handled and return them. They run after the report is sent
/// (a reboot would otherwise cut the report off).
pub fn take_new_actions(m: &Manifest, state: &mut Applied) -> Vec<valo_proto::Action> {
    let new: Vec<_> = m.actions.iter().filter(|a| !state.done_actions.contains(&a.id)).cloned().collect();
    state.done_actions.extend(new.iter().map(|a| a.id));
    let excess = state.done_actions.len().saturating_sub(200);
    state.done_actions.drain(..excess);
    new
}

pub fn run_action(p: &mut Platform, a: &valo_proto::Action) -> Result<(), String> {
    match a.kind.as_str() {
        "reboot" => p.run("systemctl", &["reboot"]).map(|_| ()),
        "poweroff" => p.run("systemctl", &["poweroff"]).map(|_| ()),
        other => Err(format!("unbekannte Aktion {other}")),
    }
}

/// Files the agent fetched for this manifest (e.g. wallpaper by sha256).
pub type Downloads = std::collections::HashMap<String, Vec<u8>>;

#[cfg(test)]
pub fn apply(p: &mut Platform, m: &Manifest, state: &mut Applied) -> Outcome {
    apply_with(p, m, state, &Downloads::new())
}

pub fn apply_with(p: &mut Platform, m: &Manifest, state: &mut Applied, files: &Downloads) -> Outcome {
    let s = &m.settings;
    let mut messages = vec![];
    let mut ok = true;
    let mut step = |name: &str, r: Result<Vec<String>, String>| match r {
        Ok(changes) => {
            for c in changes {
                messages.push(format!("{name}: {c}"));
            }
        }
        Err(e) => {
            ok = false;
            messages.push(format!("FEHLER {name}: {e}"));
        }
    };
    step("Tastatur", keyboard(p, s));
    step("Sprache", locale(p, s));
    step("Zeitzone", timezone(p, s));
    step("Firefox", firefox(p, s));
    step("Rechte", permissions(p, m));
    step("Home", home(p, s));
    step("Hintergrund", wallpaper(p, s, files));
    step("Admin-Konto", admin_account(p, m, state));
    step("Apps", apps(p, m, state));
    step("Identität", identity(p, m));
    if messages.is_empty() {
        messages.push("keine Änderungen nötig".into());
    }
    state.revision = Some(m.revision.clone());
    state.ok = ok;
    Outcome { ok, messages }
}

fn changed(c: bool, what: &str) -> Vec<String> {
    if c { vec![what.to_string()] } else { vec![] }
}

fn valid(v: &str, extra: &str) -> bool {
    !v.is_empty() && v.len() < 64 && v.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c)) && !v.contains("..")
}

// ---------------------------------------------------------------------------

pub fn keyboard(p: &mut Platform, s: &Settings) -> Result<Vec<String>, String> {
    let (layout, variant) = s.keyboard_parts();
    if !valid(&layout, "") || !(variant.is_empty() || valid(&variant, "_-")) {
        return Err(format!("ungültiges Layout {:?}", s.keyboard));
    }
    let default = format!(
        "# Managed by valo-agent\nXKBMODEL=\"pc105\"\nXKBLAYOUT=\"{layout}\"\nXKBVARIANT=\"{variant}\"\nXKBOPTIONS=\"\"\nBACKSPACE=\"guess\"\n"
    );
    let xorg = format!(
        "# Managed by valo-agent\nSection \"InputClass\"\n    Identifier \"system-keyboard\"\n    MatchIsKeyboard \"on\"\n    Option \"XkbLayout\" \"{layout}\"\n    Option \"XkbVariant\" \"{variant}\"\nEndSection\n"
    );
    let a = p.write("/etc/default/keyboard", &default, 0o644)?;
    let b = p.write("/etc/X11/xorg.conf.d/00-keyboard.conf", &xorg, 0o644)?;
    if a {
        // console keymap; X picks the new layout at the next login
        let _ = p.run("setupcon", &["--save-only"]);
    }
    Ok(changed(a || b, &format!("Layout {layout}{}", if variant.is_empty() { String::new() } else { format!(" ({variant})") })))
}

pub fn locale(p: &mut Platform, s: &Settings) -> Result<Vec<String>, String> {
    let loc = s.locale.clone().unwrap_or_else(|| "en_US.UTF-8".into());
    let Some(base) = loc.strip_suffix(".UTF-8") else { return Err(format!("nur UTF-8 unterstützt: {loc}")) };
    if !valid(base, "_") {
        return Err(format!("ungültige Sprache {loc}"));
    }
    let gen_line = format!("{loc} UTF-8");
    let current = p.read("/etc/locale.gen").unwrap_or_default();
    let mut found = false;
    let mut lines: Vec<String> = current
        .lines()
        .map(|l| {
            let bare = l.trim_start_matches('#').trim();
            if bare == gen_line {
                found = true;
                gen_line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        lines.push(gen_line);
    }
    let gen_changed = p.write("/etc/locale.gen", &(lines.join("\n") + "\n"), 0o644)?;
    if gen_changed {
        p.run("locale-gen", &[])?;
    }
    let def = p.write("/etc/default/locale", &format!("LANG={loc}\n"), 0o644)?;
    Ok(changed(gen_changed || def, &loc))
}

pub fn timezone(p: &mut Platform, s: &Settings) -> Result<Vec<String>, String> {
    let tz = s.timezone.clone().unwrap_or_else(|| "UTC".into());
    if !valid(&tz, "/_+-") || tz.starts_with('/') {
        return Err(format!("ungültige Zeitzone {tz}"));
    }
    let a = p.write("/etc/timezone", &format!("{tz}\n"), 0o644)?;
    let b = p.symlink("/etc/localtime", &format!("/usr/share/zoneinfo/{tz}"))?;
    Ok(changed(a || b, &tz))
}

/// Firefox enterprise policies. Paths and keys follow Mozilla's policy
/// templates; verify against the shipped Firefox ESR before a pilot.
pub fn firefox_policies(s: &Settings) -> Value {
    let homepage = s.browser_homepage.clone().unwrap_or_default();
    let mut policies = json!({
        "DisableAppUpdate": true,
        "DisableTelemetry": true,
        "DisableFirefoxStudies": true,
        "DontCheckDefaultBrowser": true,
        "OverrideFirstRunPage": "",
        "OverridePostUpdatePage": "",
        "PasswordManagerEnabled": false,
    });
    if homepage.starts_with("https://") || homepage.starts_with("http://") {
        policies["Homepage"] = json!({ "URL": homepage, "Locked": true, "StartPage": "homepage" });
    }
    let (name, add) = match s.browser_search.as_deref().unwrap_or("ecosia") {
        "duckduckgo" => ("DuckDuckGo", None),
        "google" => ("Google", None),
        "startpage" => (
            "Startpage",
            Some(json!({"Name": "Startpage", "URLTemplate": "https://www.startpage.com/sp/search?query={searchTerms}",
                        "IconURL": "https://www.startpage.com/favicon.ico"})),
        ),
        _ => (
            "Ecosia",
            Some(json!({"Name": "Ecosia", "URLTemplate": "https://www.ecosia.org/search?q={searchTerms}",
                        "IconURL": "https://www.ecosia.org/favicon.ico",
                        "SuggestURLTemplate": "https://ac.ecosia.org/autocomplete?q={searchTerms}"})),
        ),
    };
    let mut search = json!({ "Default": name, "PreventInstalls": true });
    if let Some(a) = add {
        search["Add"] = json!([a]);
    }
    policies["SearchEngines"] = search;

    let mut ext = serde_json::Map::new();
    if !s.browser_allow_addons.unwrap_or(false) {
        ext.insert(
            "*".into(),
            json!({ "installation_mode": "blocked", "blocked_install_message": "Erweiterungen werden zentral verwaltet." }),
        );
    }
    ext.insert(
        "uBlock0@raymondhill.net".into(),
        if s.browser_adblock.unwrap_or(true) {
            json!({ "installation_mode": "force_installed",
                    "install_url": "https://addons.mozilla.org/firefox/downloads/latest/ublock-origin/latest.xpi" })
        } else {
            json!({ "installation_mode": "blocked" })
        },
    );
    policies["ExtensionSettings"] = Value::Object(ext);

    let bookmarks: Vec<Value> = s
        .browser_bookmarks
        .iter()
        .flatten()
        .filter_map(|line| {
            let (name, url) = line.split_once('|')?;
            let (name, url) = (name.trim(), url.trim());
            (!name.is_empty() && is_web_url(url)).then(|| json!({ "name": name, "url": url }))
        })
        .collect();
    if !bookmarks.is_empty() {
        let mut list = vec![json!({ "toplevel_name": "Schule" })];
        list.extend(bookmarks);
        policies["ManagedBookmarks"] = Value::Array(list);
        policies["DisplayBookmarksToolbar"] = json!("always");
    }

    let mut sites: Vec<String> = s.browser_sites.iter().flatten().filter_map(|d| clean_domain(d)).collect();
    if s.browser_filter.as_deref() == Some("allow") {
        // homepage and search engine must stay reachable in allow-list mode
        let search = match s.browser_search.as_deref().unwrap_or("ecosia") {
            "duckduckgo" => "duckduckgo.com",
            "google" => "google.com",
            "startpage" => "startpage.com",
            _ => "ecosia.org",
        };
        sites.extend(clean_domain(&homepage));
        sites.push(search.to_string());
    }
    let mut seen = std::collections::HashSet::new();
    sites.retain(|d| seen.insert(d.clone()));
    let patterns: Vec<String> =
        sites.iter().flat_map(|d| [format!("*://{d}/*"), format!("*://*.{d}/*")]).collect();
    match s.browser_filter.as_deref() {
        Some("block") if !patterns.is_empty() => {
            policies["WebsiteFilter"] = json!({ "Block": patterns });
        }
        Some("allow") => {
            policies["WebsiteFilter"] = json!({ "Block": ["<all_urls>"], "Exceptions": patterns });
        }
        _ => {}
    }
    json!({ "policies": policies })
}

fn is_web_url(u: &str) -> bool {
    (u.starts_with("https://") || u.starts_with("http://")) && !u.contains(char::is_whitespace) && u.len() < 2000
}

/// "https://www.Example.ch/x" -> "example.ch"; None if it is not a plain domain.
pub fn clean_domain(input: &str) -> Option<String> {
    let mut d = input.trim().to_lowercase();
    for prefix in ["https://", "http://"] {
        if let Some(rest) = d.strip_prefix(prefix) {
            d = rest.to_string();
        }
    }
    let d = d.split('/').next().unwrap_or("").trim_start_matches("www.").trim_start_matches("*.").to_string();
    let ok = d.contains('.')
        && d.len() < 254
        && d.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    ok.then_some(d)
}

pub fn firefox(p: &mut Platform, s: &Settings) -> Result<Vec<String>, String> {
    let text = serde_json::to_string_pretty(&firefox_policies(s)).expect("json") + "\n";
    let c = p.write("/etc/firefox/policies/policies.json", &text, 0o644)?;
    Ok(changed(c, "Richtlinien aktualisiert"))
}

pub fn permissions(p: &mut Platform, m: &Manifest) -> Result<Vec<String>, String> {
    let s = &m.settings;
    let allow_install = s.allow_app_install.unwrap_or(false);
    let perms = json!({
        "role": m.role,
        "allow_app_install": allow_install,
        "allow_user_settings": s.allow_user_settings.unwrap_or(false),
        "allow_lock_screen": s.allow_lock_screen.unwrap_or(true),
    });
    let a = p.write(
        "/etc/valolinux/permissions.json",
        &(serde_json::to_string_pretty(&perms).expect("json") + "\n"),
        0o644,
    )?;
    // Flatpak installs by the kiosk user. NOTE: allowing this lets the user install
    // anything from the configured remotes, not only catalog apps (needs our own
    // store front-end later).
    let result = if allow_install { "polkit.Result.YES" } else { "polkit.Result.NO" };
    let rule = format!(
        "// Managed by valo-agent\npolkit.addRule(function(action, subject) {{\n    \
         if (action.id.indexOf(\"org.freedesktop.Flatpak.\") == 0 && subject.user == \"{KIOSK_USER}\") {{\n        \
         return {result};\n    }}\n}});\n"
    );
    let b = p.write("/etc/polkit-1/rules.d/50-valolinux.rules", &rule, 0o644)?;
    Ok(changed(a || b, &format!("Rolle {}, Apps installieren: {}", m.role, if allow_install { "ja" } else { "nein" })))
}

pub fn home(p: &mut Platform, s: &Settings) -> Result<Vec<String>, String> {
    let mode = match s.home_mode.unwrap_or(HomeMode::Wipe) {
        HomeMode::Wipe => "wipe",
        HomeMode::Keep => "keep",
    };
    // read by the logout hook (valo-home-cleanup) in the image
    let c = p.write("/etc/valolinux/home-mode", &format!("{mode}\n"), 0o644)?;
    Ok(changed(c, if mode == "wipe" { "beim Abmelden löschen" } else { "behalten" }))
}

pub const DEFAULT_WALLPAPER: &str = "/usr/share/backgrounds/valolinux.png";

/// Desktop wallpaper for new sessions (FlameWM settings in /etc/skel; the
/// kiosk home is recreated from skel at logout when home mode is "wipe").
pub fn wallpaper(p: &mut Platform, s: &Settings, files: &Downloads) -> Result<Vec<String>, String> {
    let choice = s.wallpaper.clone().unwrap_or_else(|| "default".into());
    let path = if valo_proto::is_sha256(&choice) {
        let data = files.get(&choice).ok_or("Bild noch nicht heruntergeladen")?;
        if valo_proto::sha256_hex(data) != choice {
            return Err("Prüfsumme des Bildes stimmt nicht".into());
        }
        let ext = valo_proto::image_kind(data).ok_or("kein PNG/JPEG")?;
        let path = format!("/usr/share/backgrounds/valolinux-custom-{}.{ext}", &choice[..16]);
        p.write_bytes(&path, data, 0o644)?;
        path
    } else {
        DEFAULT_WALLPAPER.to_string()
    };
    let c = p.write("/etc/skel/.config/flamewm/settings.toml", &format!("wallpaper={path}\n"), 0o644)?;
    Ok(changed(c, if path == DEFAULT_WALLPAPER { "Standardbild" } else { "eigenes Bild" }))
}

pub fn admin_account(p: &mut Platform, m: &Manifest, state: &mut Applied) -> Result<Vec<String>, String> {
    let Some(hash) = &m.admin_password_hash else { return Ok(vec![]) };
    if !hash.starts_with("$6$") || hash.contains(['\n', ':', ' ']) {
        return Err("ungültiger Passwort-Hash".into());
    }
    if state.admin_hash.as_deref() == Some(hash) {
        return Ok(vec![]);
    }
    let mut out = vec![];
    if !p.check("id", &[ADMIN_USER]) {
        p.run("useradd", &["--create-home", "--shell", "/bin/bash", "--groups", "sudo", ADMIN_USER])?;
        out.push(format!("Benutzer {ADMIN_USER} angelegt"));
    }
    p.run("usermod", &["--password", hash, ADMIN_USER])?;
    state.admin_hash = Some(hash.clone());
    out.push("Passwort gesetzt".into());
    Ok(out)
}

fn valid_ref(r: &str) -> bool {
    let parts: Vec<&str> = r.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
}

pub fn apps(p: &mut Platform, m: &Manifest, state: &mut Applied) -> Result<Vec<String>, String> {
    let wanted: Vec<String> = m.apps.iter().map(|a| a.flatpak_ref.clone()).filter(|r| valid_ref(r)).collect();
    let to_install: Vec<&String> = wanted.iter().filter(|r| !state.managed_apps.contains(r)).collect();
    let to_remove: Vec<String> = state.managed_apps.iter().filter(|r| !wanted.contains(r)).cloned().collect();
    if to_install.is_empty() && to_remove.is_empty() {
        return Ok(vec![]);
    }
    let mut out = vec![];
    let mut errors = vec![];
    if !to_install.is_empty() {
        p.run(
            "flatpak",
            &["remote-add", "--system", "--if-not-exists", "flathub", "https://dl.flathub.org/repo/flathub.flatpakrepo"],
        )?;
    }
    for r in to_install {
        match p.run("flatpak", &["install", "--system", "--noninteractive", "-y", "flathub", r]) {
            Ok(_) => {
                state.managed_apps.push(r.clone());
                out.push(format!("{r} installiert"));
            }
            Err(e) => errors.push(e),
        }
    }
    for r in to_remove {
        match p.run("flatpak", &["uninstall", "--system", "--noninteractive", "-y", &r]) {
            Ok(_) => {
                state.managed_apps.retain(|x| x != &r);
                out.push(format!("{r} entfernt"));
            }
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() { Ok(out) } else { Err(errors.join("; ")) }
}

/// Device name as hostname (lowercase, a-z0-9-).
pub fn identity(p: &mut Platform, m: &Manifest) -> Result<Vec<String>, String> {
    let host: String = m
        .device_name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'ä' => 'a',
            'ö' => 'o',
            'ü' => 'u',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let host: String = host.chars().take(63).collect();
    if host.is_empty() {
        return Ok(vec![]);
    }
    let c = p.write("/etc/hostname", &format!("{host}\n"), 0o644)?;
    if c {
        let _ = p.run("hostname", &[&host]);
    }
    Ok(changed(c, &format!("Hostname {host}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use valo_proto::CatalogApp;

    fn manifest(settings: Settings, apps: Vec<&str>) -> Manifest {
        Manifest {
            revision: "r1".into(),
            device_name: "Zimmer 12 – PC 3".into(),
            role: "student".into(),
            group: None,
            settings: Settings::resolve([&settings]),
            admin_password_hash: Some("$6$salt$hash".into()),
            apps: apps
                .into_iter()
                .map(|r| CatalogApp { id: r.into(), name: r.into(), flatpak_ref: r.into() })
                .collect(),
            actions: vec![],
            issued_at: 0,
        }
    }

    #[test]
    fn applies_files_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("etc")).unwrap();
        std::fs::write(dir.path().join("etc/locale.gen"), "# en_US.UTF-8 UTF-8\n# de_CH.UTF-8 UTF-8\n").unwrap();
        let mut p = Platform::new(dir.path(), true);
        let mut st = Applied::default();
        let m = manifest(Settings { keyboard: Some("ch:fr".into()), ..Default::default() }, vec!["org.gimp.GIMP"]);

        let out = apply(&mut p, &m, &mut st);
        assert!(out.ok, "{:?}", out.messages);
        let read = |f: &str| std::fs::read_to_string(dir.path().join(f)).unwrap();
        assert!(read("etc/default/keyboard").contains("XKBLAYOUT=\"ch\"\nXKBVARIANT=\"fr\""));
        assert!(read("etc/X11/xorg.conf.d/00-keyboard.conf").contains("\"XkbVariant\" \"fr\""));
        assert_eq!(read("etc/default/locale"), "LANG=de_CH.UTF-8\n");
        assert_eq!(read("etc/locale.gen"), "# en_US.UTF-8 UTF-8\nde_CH.UTF-8 UTF-8\n");
        assert_eq!(read("etc/timezone"), "Europe/Zurich\n");
        assert_eq!(
            std::fs::read_link(dir.path().join("etc/localtime")).unwrap().to_str().unwrap(),
            "/usr/share/zoneinfo/Europe/Zurich"
        );
        assert_eq!(read("etc/hostname"), "zimmer-12-pc-3\n");
        assert_eq!(read("etc/valolinux/home-mode"), "wipe\n");
        assert!(read("etc/polkit-1/rules.d/50-valolinux.rules").contains("polkit.Result.NO"));
        let ff: Value = serde_json::from_str(&read("etc/firefox/policies/policies.json")).unwrap();
        assert_eq!(ff["policies"]["SearchEngines"]["Default"], "Ecosia");
        assert_eq!(ff["policies"]["ExtensionSettings"]["*"]["installation_mode"], "blocked");
        assert_eq!(st.managed_apps, vec!["org.gimp.GIMP"]);
        assert!(p.log.iter().any(|l| l.contains("usermod --password $6$salt$hash valoadmin")));
        assert!(p.log.iter().any(|l| l.contains("flatpak install --system --noninteractive -y flathub org.gimp.GIMP")));

        // second run: nothing to do
        let mut p2 = Platform::new(dir.path(), true);
        let out2 = apply(&mut p2, &m, &mut st);
        assert_eq!(out2.messages, vec!["keine Änderungen nötig"]);
        assert!(!p2.log.iter().any(|l| l.starts_with("run")), "{:?}", p2.log);

        // app removed from manifest -> uninstalled
        let m3 = manifest(Settings { keyboard: Some("ch:fr".into()), ..Default::default() }, vec![]);
        let mut p3 = Platform::new(dir.path(), true);
        apply(&mut p3, &m3, &mut st);
        assert!(p3.log.iter().any(|l| l.contains("flatpak uninstall") && l.contains("org.gimp.GIMP")));
        assert!(st.managed_apps.is_empty());
    }

    #[test]
    fn actions_run_once() {
        let mut m = manifest(Settings::default(), vec![]);
        m.actions = vec![valo_proto::Action { id: 7, kind: "reboot".into() }];
        let mut st = Applied::default();
        assert_eq!(take_new_actions(&m, &mut st).len(), 1);
        assert!(take_new_actions(&m, &mut st).is_empty());
        let dir = tempfile::tempdir().unwrap();
        let mut p = Platform::new(dir.path(), true);
        run_action(&mut p, &m.actions[0]).unwrap();
        assert_eq!(p.log.last().unwrap(), "run systemctl reboot");
        assert!(run_action(&mut p, &valo_proto::Action { id: 8, kind: "format".into() }).is_err());
    }

    #[test]
    fn wallpaper_checks_hash() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Platform::new(dir.path(), true);
        let img = b"\x89PNG\r\n\x1a\nrest-of-image".to_vec();
        let sha = valo_proto::sha256_hex(&img);
        let s = Settings::resolve([&Settings { wallpaper: Some(sha.clone()), ..Default::default() }]);
        let mut files = Downloads::new();
        assert!(wallpaper(&mut p, &s, &files).is_err(), "missing download");
        files.insert(sha.clone(), b"\x89PNG\r\n\x1a\ntampered".to_vec());
        assert!(wallpaper(&mut p, &s, &files).is_err(), "hash mismatch");
        files.insert(sha.clone(), img.clone());
        wallpaper(&mut p, &s, &files).unwrap();
        let skel = std::fs::read_to_string(dir.path().join("etc/skel/.config/flamewm/settings.toml")).unwrap();
        let target = format!("/usr/share/backgrounds/valolinux-custom-{}.png", &sha[..16]);
        assert_eq!(skel, format!("wallpaper={target}\n"));
        assert_eq!(std::fs::read(dir.path().join(target.trim_start_matches('/'))).unwrap(), img);
        wallpaper(&mut p, &Settings::defaults(), &files).unwrap();
        let skel = std::fs::read_to_string(dir.path().join("etc/skel/.config/flamewm/settings.toml")).unwrap();
        assert_eq!(skel, format!("wallpaper={DEFAULT_WALLPAPER}\n"));
    }

    #[test]
    fn rejects_bad_values() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Platform::new(dir.path(), true);
        let bad = Settings::resolve([&Settings {
            keyboard: Some("ch; rm -rf /".into()),
            timezone: Some("../../etc/shadow".into()),
            ..Default::default()
        }]);
        assert!(keyboard(&mut p, &bad).is_err());
        assert!(timezone(&mut p, &bad).is_err());
        let mut m = manifest(Settings::default(), vec!["org.ok.App", "bad ref; reboot"]);
        m.admin_password_hash = Some("$6$x\nroot::0".into());
        let mut st = Applied::default();
        assert!(admin_account(&mut p, &m, &mut st).is_err());
        apps(&mut p, &m, &mut st).unwrap();
        assert_eq!(st.managed_apps, vec!["org.ok.App"]);
    }

    #[test]
    fn firefox_filter_and_bookmarks() {
        let s = Settings::resolve([&Settings {
            browser_bookmarks: Some(vec![
                "Schule | https://www.schule.ch".into(),
                "kaputt".into(),
                "JS | javascript:alert(1)".into(),
            ]),
            browser_filter: Some("allow".into()),
            browser_sites: Some(vec!["https://www.Schule.ch/home".into(), "bad domain".into(), "edu.ch".into()]),
            ..Default::default()
        }]);
        let v = firefox_policies(&s);
        let bm = v["policies"]["ManagedBookmarks"].as_array().unwrap();
        assert_eq!(bm.len(), 2, "toplevel + one valid bookmark");
        assert_eq!(bm[1]["url"], "https://www.schule.ch");
        let f = &v["policies"]["WebsiteFilter"];
        assert_eq!(f["Block"][0], "<all_urls>");
        assert_eq!(
            f["Exceptions"],
            json!(["*://schule.ch/*", "*://*.schule.ch/*", "*://edu.ch/*", "*://*.edu.ch/*",
                   "*://ecosia.org/*", "*://*.ecosia.org/*"]),
            "homepage + search engine added once"
        );
        let off = firefox_policies(&Settings::defaults());
        assert!(off["policies"].get("WebsiteFilter").is_none());
        assert!(off["policies"].get("ManagedBookmarks").is_none());
    }

    #[test]
    fn firefox_policy_variants() {
        let s = Settings::resolve([&Settings {
            browser_search: Some("duckduckgo".into()),
            browser_adblock: Some(false),
            browser_allow_addons: Some(true),
            browser_homepage: Some("javascript:alert(1)".into()),
            ..Default::default()
        }]);
        let v = firefox_policies(&s);
        assert_eq!(v["policies"]["SearchEngines"]["Default"], "DuckDuckGo");
        assert!(v["policies"]["SearchEngines"].get("Add").is_none());
        assert!(v["policies"]["ExtensionSettings"].get("*").is_none());
        assert_eq!(v["policies"]["ExtensionSettings"]["uBlock0@raymondhill.net"]["installation_mode"], "blocked");
        assert!(v["policies"].get("Homepage").is_none(), "non-http homepage is ignored");
    }
}
