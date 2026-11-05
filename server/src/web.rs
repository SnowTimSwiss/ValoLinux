//! Admin web UI (server-rendered HTML, no JavaScript framework).

use crate::auth::{self, Admin};
use crate::db::{CatalogEntry, Device};
use crate::fields::{self, Field, Kind, SECTIONS};
use crate::util::{self, FormData, ago, format_code, is_online, url_encode};
use crate::Shared;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use maud::{DOCTYPE, Markup, html};
use serde_json::Value;
use std::collections::HashMap;
use valo_proto::Settings;

type Q = Query<HashMap<String, String>>;
type Page = Result<Response, Response>;

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/", get(dashboard))
        .route("/static/style.css", get(style))
        .route("/static/logo.svg", get(logo))
        .route("/setup", get(setup_page).post(setup_submit))
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", post(logout))
        .route("/devices", get(devices_page))
        .route("/devices/{id}", get(device_page).post(device_update))
        .route("/devices/{id}/password", post(device_password))
        .route("/devices/{id}/delete", post(device_delete))
        .route("/adopt", get(adopt_page).post(adopt_submit))
        .route("/groups", get(groups_page).post(group_create))
        .route("/groups/{id}/delete", post(group_delete))
        .route("/settings", get(|| async { Redirect::to("/settings/global") }))
        .route("/settings/{scope}", get(settings_page).post(settings_submit))
        .route("/apps", get(apps_page).post(app_create))
        .route("/apps/{id}/delete", post(app_delete))
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn e500<E: std::fmt::Display>(e: E) -> Response {
    tracing::error!("internal error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, "Interner Fehler, siehe Server-Log.").into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Nicht gefunden").into_response()
}

fn redirect_msg(path: &str, msg: &str) -> Response {
    let sep = if path.contains('?') { '&' } else { '?' };
    Redirect::to(&format!("{path}{sep}msg={}", url_encode(msg))).into_response()
}

fn redirect_err(path: &str, msg: &str) -> Response {
    let sep = if path.contains('?') { '&' } else { '?' };
    Redirect::to(&format!("{path}{sep}err={}", url_encode(msg))).into_response()
}

fn csrf(admin: &Admin) -> Markup {
    html! { input type="hidden" name="csrf" value=(admin.csrf); }
}

fn role_label(role: &str) -> &'static str {
    match role {
        "teacher" => "Lehrperson",
        _ => "Schüler/in",
    }
}

/// Settings scope as used in URLs: "global", "role:student", "group:3", "device:7".
enum Scope {
    Global,
    Role(&'static str),
    Group(i64),
    Device(i64),
}

impl Scope {
    fn parse(s: &str) -> Option<Scope> {
        match s.split_once(':') {
            None if s == "global" => Some(Scope::Global),
            Some(("role", "student")) => Some(Scope::Role("student")),
            Some(("role", "teacher")) => Some(Scope::Role("teacher")),
            Some(("group", id)) => id.parse().ok().map(Scope::Group),
            Some(("device", id)) => id.parse().ok().map(Scope::Device),
            _ => None,
        }
    }
    fn key(&self) -> String {
        match self {
            Scope::Global => "global".into(),
            Scope::Role(r) => format!("role:{r}"),
            Scope::Group(id) => format!("group:{id}"),
            Scope::Device(id) => format!("device:{id}"),
        }
    }
}

fn scope_label(s: &Shared, scope: &str) -> String {
    match Scope::parse(scope) {
        Some(Scope::Global) => "Global".into(),
        Some(Scope::Role(r)) => format!("Rolle {}", role_label(r)),
        Some(Scope::Group(id)) => {
            s.db.group(id).ok().flatten().map(|g| format!("Gruppe {}", g.name)).unwrap_or("Gruppe".into())
        }
        Some(Scope::Device(_)) => "Gerät".into(),
        None => scope.into(),
    }
}

// ---------------------------------------------------------------------------
// layout
// ---------------------------------------------------------------------------

fn flash(q: &HashMap<String, String>) -> Markup {
    html! {
        @if let Some(m) = q.get("msg") { div.flash.ok { (m) } }
        @if let Some(m) = q.get("err") { div.flash.err { (m) } }
    }
}

fn head(title: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="utf-8";
        meta name="viewport" content="width=device-width, initial-scale=1";
        title { (title) " · ValoLinux" }
        link rel="icon" href="/static/logo.svg";
        link rel="stylesheet" href="/static/style.css";
    }
}

fn layout(s: &Shared, admin: &Admin, active: &str, title: &str, q: &HashMap<String, String>, body: Markup) -> Response {
    let pending = s.db.devices().map(|d| d.iter().filter(|d| !d.adopted()).count()).unwrap_or(0);
    let nav = [
        ("/", "Übersicht", "◉"),
        ("/devices", "Geräte", "▭"),
        ("/adopt", "Hinzufügen", "＋"),
        ("/groups", "Gruppen", "◫"),
        ("/settings", "Einstellungen", "⚙"),
        ("/apps", "App-Katalog", "▦"),
    ];
    html! {
        (head(title))
        body {
            div.shell {
                aside.sidebar {
                    a.brand href="/" { img src="/static/logo.svg" alt=""; span { b { "valo" } "linux" } }
                    nav {
                        @for (href, label, icon) in nav {
                            a.active[active == href] href=(href) {
                                span.icon { (icon) } (label)
                                @if href == "/adopt" && pending > 0 { span.badge { (pending) } }
                            }
                        }
                    }
                    div.sidebar-foot {
                        span.user { "Angemeldet als " b { (admin.username) } }
                        form method="post" action="/logout" { (csrf(admin)) button.link type="submit" { "Abmelden" } }
                    }
                }
                main.content {
                    h1 { (title) }
                    (flash(q))
                    (body)
                }
            }
        }
    }
    .into_response()
}

async fn style() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], include_str!("style.css"))
}

async fn logo() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/svg+xml")], include_str!("logo.svg"))
}

// ---------------------------------------------------------------------------
// setup & login
// ---------------------------------------------------------------------------

fn auth_page(title: &str, intro: &str, action: &str, button: &str, err: Option<&str>, confirm: bool) -> Response {
    html! {
        (head(title))
        body.auth {
            div.auth-card {
                img.auth-logo src="/static/logo.svg" alt="";
                h1 { b { "valo" } "linux" }
                p.muted { (intro) }
                @if let Some(e) = err { div.flash.err { (e) } }
                form method="post" action=(action) {
                    label { "Benutzername" input name="username" required autofocus autocomplete="username"; }
                    label { "Passwort" input type="password" name="password" required minlength="10"
                        autocomplete=(if confirm { "new-password" } else { "current-password" }); }
                    @if confirm {
                        label { "Passwort wiederholen" input type="password" name="password2" required minlength="10" autocomplete="new-password"; }
                    }
                    button.primary.wide type="submit" { (button) }
                }
            }
        }
    }
    .into_response()
}

async fn setup_page(State(s): State<Shared>) -> Response {
    if s.db.admin_count().unwrap_or(0) > 0 {
        return Redirect::to("/login").into_response();
    }
    auth_page(
        "Einrichten",
        "Willkommen! Lege das erste Admin-Konto an (mindestens 10 Zeichen).",
        "/setup",
        "Konto anlegen",
        None,
        true,
    )
}

async fn setup_submit(State(s): State<Shared>, body: String) -> Page {
    if s.db.admin_count().map_err(e500)? > 0 {
        return Ok(Redirect::to("/login").into_response());
    }
    let f = FormData::parse(&body);
    let (user, pw) = (f.text("username"), f.get("password").unwrap_or(""));
    let fail = |m| auth_page("Einrichten", "Lege das erste Admin-Konto an.", "/setup", "Konto anlegen", Some(m), true);
    if user.is_empty() || pw.chars().count() < 10 {
        return Ok(fail("Benutzername fehlt oder Passwort kürzer als 10 Zeichen."));
    }
    if Some(pw) != f.get("password2") {
        return Ok(fail("Die Passwörter stimmen nicht überein."));
    }
    let id = s.db.create_admin(&user, &auth::hash_password(pw)).map_err(e500)?;
    let cookie = auth::login_cookie(&s, id).map_err(e500)?;
    Ok(([(header::SET_COOKIE, cookie)], Redirect::to("/")).into_response())
}

async fn login_page(State(s): State<Shared>) -> Response {
    if s.db.admin_count().unwrap_or(0) == 0 {
        return Redirect::to("/setup").into_response();
    }
    auth_page("Anmelden", "Verwaltung der Schulgeräte", "/login", "Anmelden", None, false)
}

async fn login_submit(State(s): State<Shared>, body: String) -> Page {
    let f = FormData::parse(&body);
    let user = f.text("username");
    let pw = f.get("password").unwrap_or("");
    if let Some((id, hash)) = s.db.admin_by_name(&user).map_err(e500)? {
        if auth::verify_password(pw, &hash) {
            let cookie = auth::login_cookie(&s, id).map_err(e500)?;
            return Ok(([(header::SET_COOKIE, cookie)], Redirect::to("/")).into_response());
        }
    } else {
        // spend comparable time for unknown users
        let _ = auth::verify_password(pw, "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    }
    tracing::warn!("failed login for {user:?}");
    Ok(auth_page("Anmelden", "Verwaltung der Schulgeräte", "/login", "Anmelden", Some("Benutzername oder Passwort falsch."), false))
}

async fn logout(State(s): State<Shared>, admin: Admin, headers: HeaderMap, body: String) -> Page {
    admin.check_csrf(&FormData::parse(&body))?;
    if let Some(t) = auth::session_token(&headers) {
        s.db.delete_session(&t).map_err(e500)?;
    }
    Ok(([(header::SET_COOKIE, auth::logout_cookie())], Redirect::to("/login")).into_response())
}

// ---------------------------------------------------------------------------
// dashboard
// ---------------------------------------------------------------------------

fn status_pill(d: &Device) -> Markup {
    let (class, text) = if !d.adopted() {
        ("wait", "wartet")
    } else if d.last_ok == Some(false) {
        ("bad", "Fehler")
    } else if is_online(d.last_seen) {
        ("good", "online")
    } else {
        ("off", "offline")
    };
    html! { span class={"pill " (class)} { span.dot {} (text) } }
}

fn code_form(admin: &Admin, big: bool) -> Markup {
    html! {
        form.codeform.big[big] method="get" action="/adopt" {
            input.code name="code" placeholder="ABC-123" maxlength="8" autocomplete="off" spellcheck="false"
                aria-label="Code vom Gerät";
            button.primary type="submit" { "Gerät hinzufügen" }
        }
        @let _ = admin;
    }
}

async fn dashboard(State(s): State<Shared>, admin: Admin, Query(q): Q) -> Page {
    let devices = s.db.devices().map_err(e500)?;
    let adopted: Vec<_> = devices.iter().filter(|d| d.adopted()).collect();
    let pending: Vec<_> = devices.iter().filter(|d| !d.adopted()).collect();
    let online = adopted.iter().filter(|d| is_online(d.last_seen)).count();
    let errors: Vec<_> = adopted.iter().filter(|d| d.last_ok == Some(false)).collect();
    let teachers = adopted.iter().filter(|d| d.role == "teacher").count();
    let body = html! {
        div.stats {
            a.stat href="/devices" { span.num { (adopted.len()) } span.lbl { "Geräte" } span.sub { (teachers) " Lehrpersonen · " (adopted.len() - teachers) " Schüler/innen" } }
            a.stat href="/devices" { span.num.good { (online) } span.lbl { "Online" } span.sub { "in den letzten 10 Minuten" } }
            a.stat href="/adopt" { span.num.warn { (pending.len()) } span.lbl { "Warten auf Freigabe" } span.sub { "neue Geräte" } }
            a.stat href="/devices" { span.num.bad[!errors.is_empty()] { (errors.len()) } span.lbl { "Mit Fehlern" } span.sub { "letzte Anwendung" } }
        }
        section.card.hero {
            div {
                h2 { "Neues Gerät hinzufügen" }
                p.muted { "Gerät mit ValoLinux starten. Es zeigt einen Code an. Den Code hier eingeben, Rolle wählen, fertig." }
            }
            (code_form(&admin, true))
        }
        @if !errors.is_empty() {
            section.card {
                h2 { "Geräte mit Fehlern" }
                (device_table(&errors.iter().map(|d| (**d).clone()).collect::<Vec<_>>()))
            }
        }
        section.card {
            div.card-head { h2 { "Zuletzt gesehen" } a.button href="/devices" { "Alle Geräte" } }
            @let mut recent: Vec<Device> = adopted.iter().map(|d| (*d).clone()).collect();
            @let _ = recent.sort_by_key(|d| std::cmp::Reverse(d.last_seen));
            @if recent.is_empty() {
                p.empty { "Noch keine Geräte. Starte ein Gerät mit ValoLinux und füge es oben mit dem Code hinzu." }
            } @else {
                (device_table(&recent.into_iter().take(8).collect::<Vec<_>>()))
            }
        }
    };
    Ok(layout(&s, &admin, "/", "Übersicht", &q, body))
}

fn device_table(devices: &[Device]) -> Markup {
    html! {
        table.list {
            thead { tr { th { "Gerät" } th { "Rolle" } th { "Gruppe" } th { "Status" } th { "Zuletzt gesehen" } } }
            tbody {
                @for d in devices {
                    tr {
                        td { a href={"/devices/" (d.id)} { b { (d.display_name()) } } br; span.muted.small { (d.hostname) } }
                        td { @if d.adopted() { span class={"role " (d.role)} { (role_label(&d.role)) } } @else { "–" } }
                        td { (d.group_name.as_deref().unwrap_or("–")) }
                        td { (status_pill(d)) }
                        td.muted { (ago(d.last_seen)) }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// devices
// ---------------------------------------------------------------------------

async fn devices_page(State(s): State<Shared>, admin: Admin, Query(q): Q) -> Page {
    let groups = s.db.groups().map_err(e500)?;
    let filter_group: Option<i64> = q.get("group").and_then(|g| g.parse().ok());
    let filter_role = q.get("role").cloned().unwrap_or_default();
    let devices: Vec<Device> = s
        .db
        .devices()
        .map_err(e500)?
        .into_iter()
        .filter(|d| d.adopted())
        .filter(|d| filter_group.is_none() || d.group_id == filter_group)
        .filter(|d| filter_role.is_empty() || d.role == filter_role)
        .collect();
    let body = html! {
        form.filters method="get" {
            select name="group" onchange="this.form.submit()" {
                option value="" { "Alle Gruppen" }
                @for g in &groups { option value=(g.id) selected[filter_group == Some(g.id)] { (g.name) } }
            }
            select name="role" onchange="this.form.submit()" {
                option value="" { "Alle Rollen" }
                option value="student" selected[filter_role == "student"] { "Schüler/innen" }
                option value="teacher" selected[filter_role == "teacher"] { "Lehrpersonen" }
            }
            noscript { button type="submit" { "Filtern" } }
            span.spacer {}
            a.button.primary href="/adopt" { "＋ Gerät hinzufügen" }
        }
        section.card {
            @if devices.is_empty() { p.empty { "Keine Geräte gefunden." } } @else { (device_table(&devices)) }
        }
    };
    Ok(layout(&s, &admin, "/devices", "Geräte", &q, body))
}

fn group_select(groups: &[crate::db::Group], selected: Option<i64>) -> Markup {
    html! {
        select name="group" {
            option value="" { "Keine Gruppe" }
            @for g in groups { option value=(g.id) selected[selected == Some(g.id)] { (g.name) } }
        }
    }
}

fn role_choice(selected: &str) -> Markup {
    html! {
        div.segmented {
            label { input type="radio" name="role" value="student" checked[selected != "teacher"]; span { "Schüler/in" } }
            label { input type="radio" name="role" value="teacher" checked[selected == "teacher"]; span { "Lehrperson" } }
        }
    }
}

async fn device_page(State(s): State<Shared>, admin: Admin, Path(id): Path<i64>, Query(q): Q) -> Page {
    let d = s.db.device(id).map_err(e500)?.ok_or_else(not_found)?;
    if !d.adopted() {
        return Ok(Redirect::to(&format!("/adopt?code={}", d.code)).into_response());
    }
    let groups = s.db.groups().map_err(e500)?;
    let catalog = s.db.catalog().map_err(e500)?;
    let layers = s.db.layers_for(&d).map_err(e500)?;
    let current_rev = crate::api::build_manifest(&s, &d)
        .ok()
        .and_then(|m| serde_json::from_str::<valo_proto::Manifest>(&m.payload).ok())
        .map(|m| m.revision);
    let in_sync = current_rev.is_some() && current_rev == d.last_revision;

    // effective value + which layer set it
    let mut effective: Vec<(&Field, Option<Value>, String)> = vec![];
    let defaults = fields::to_map(&Settings::defaults());
    for f in SECTIONS.iter().flat_map(|s| s.fields.iter()) {
        let mut val = defaults.get(f.key).cloned();
        let mut src = "Standard".to_string();
        for (scope, l) in &layers {
            if let Some(v) = fields::to_map(l).get(f.key) {
                val = Some(v.clone());
                src = scope_label(&s, scope);
            }
        }
        effective.push((f, val, src));
    }

    let body = html! {
        div.page-head {
            div { (status_pill(&d)) span.muted { " zuletzt gesehen " (ago(d.last_seen)) } }
            a.button href={"/settings/device:" (d.id)} { "⚙ Einstellungen nur für dieses Gerät" }
        }
        div.grid2 {
            section.card {
                h2 { "Zuweisung" }
                form method="post" action={"/devices/" (d.id)} {
                    (csrf(&admin))
                    label { "Name" input name="name" value=(d.name) placeholder=(d.hostname) maxlength="80"; }
                    div.field { span.label { "Rolle" } (role_choice(&d.role)) }
                    label { "Gruppe / Klasse" (group_select(&groups, d.group_id)) }
                    button.primary type="submit" { "Speichern" }
                }
            }
            section.card {
                h2 { "Lokales Admin-Konto" }
                p.muted { "Benutzer " code { "valoadmin" } ". Jedes Gerät hat ein eigenes, zufälliges Passwort." }
                details.secret {
                    summary { "Passwort anzeigen" }
                    code.password { (d.admin_password.as_deref().unwrap_or("–")) }
                }
                form method="post" action={"/devices/" (d.id) "/password"}
                    onsubmit="return confirm('Neues Passwort erzeugen? Das alte gilt nach dem nächsten Abgleich nicht mehr.')" {
                    (csrf(&admin))
                    button type="submit" { "Neues Passwort erzeugen" }
                }
            }
            section.card {
                h2 { "Zustand" }
                dl.kv {
                    dt { "Konfiguration" }
                    dd { @if in_sync { span.pill.good { span.dot {} "aktuell" } } @else { span.pill.wait { span.dot {} "wird beim nächsten Abgleich übernommen" } } }
                    dt { "Letzte Anwendung" }
                    dd { @match d.last_ok { Some(true) => "erfolgreich", Some(false) => "mit Fehlern", None => "noch keine" } }
                }
                @if !d.last_messages.is_empty() {
                    pre.log { (d.last_messages) }
                }
            }
            section.card {
                h2 { "Gerät" }
                dl.kv {
                    dt { "Hostname" } dd { (d.hostname) }
                    dt { "System" } dd { (d.os) }
                    dt { "Hardware" } dd { (d.hardware) }
                    dt { "Agent" } dd { (d.agent_version) }
                    dt { "Schlüssel" } dd { code.small { (d.public_key.chars().take(16).collect::<String>()) "…" } }
                    dt { "Hinzugefügt" } dd { (ago(Some(d.created))) }
                }
            }
        }
        section.card {
            div.card-head {
                h2 { "Wirksame Einstellungen" }
                span.muted.small { "Reihenfolge: Standard → Global → Rolle → Gruppe → Gerät" }
            }
            table.list.compact {
                tbody {
                    @for (f, v, src) in &effective {
                        tr { td { (f.label) } td { b { (fields::display(f, v.as_ref(), &catalog)) } } td { span.source { (src) } } }
                    }
                }
            }
        }
        section.card.danger-zone {
            h2 { "Gerät entfernen" }
            p.muted { "Das Gerät muss danach neu hinzugefügt werden (neu flashen oder Agent zurücksetzen)." }
            form method="post" action={"/devices/" (d.id) "/delete"} onsubmit="return confirm('Gerät wirklich entfernen?')" {
                (csrf(&admin))
                button.danger type="submit" { "Gerät entfernen" }
            }
        }
    };
    Ok(layout(&s, &admin, "/devices", d.display_name(), &q, body))
}

fn parse_group(f: &FormData) -> Option<i64> {
    f.text("group").parse().ok()
}

fn parse_role(f: &FormData) -> &'static str {
    if f.text("role") == "teacher" { "teacher" } else { "student" }
}

async fn device_update(State(s): State<Shared>, admin: Admin, Path(id): Path<i64>, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    let d = s.db.device(id).map_err(e500)?.ok_or_else(not_found)?;
    let name: String = f.text("name").chars().take(80).collect();
    s.db.update_device(d.id, &name, parse_role(&f), parse_group(&f)).map_err(e500)?;
    Ok(redirect_msg(&format!("/devices/{id}"), "Gespeichert. Das Gerät übernimmt die Änderung beim nächsten Abgleich."))
}

async fn device_password(State(s): State<Shared>, admin: Admin, Path(id): Path<i64>, body: String) -> Page {
    admin.check_csrf(&FormData::parse(&body))?;
    s.db.device(id).map_err(e500)?.ok_or_else(not_found)?;
    let pw = util::admin_password();
    s.db.set_admin_password(id, &pw, &util::crypt_sha512(&pw)).map_err(e500)?;
    Ok(redirect_msg(&format!("/devices/{id}"), "Neues Admin-Passwort erzeugt."))
}

async fn device_delete(State(s): State<Shared>, admin: Admin, Path(id): Path<i64>, body: String) -> Page {
    admin.check_csrf(&FormData::parse(&body))?;
    s.db.delete_device(id).map_err(e500)?;
    Ok(redirect_msg("/devices", "Gerät entfernt."))
}

// ---------------------------------------------------------------------------
// adoption
// ---------------------------------------------------------------------------

async fn adopt_page(State(s): State<Shared>, admin: Admin, Query(q): Q) -> Page {
    let groups = s.db.groups().map_err(e500)?;
    let pending: Vec<Device> = s.db.devices().map_err(e500)?.into_iter().filter(|d| !d.adopted()).collect();
    let code = q.get("code").map(|c| util::normalize_code(c)).unwrap_or_default();
    let found = if code.is_empty() { None } else { s.db.pending_by_code(&code).map_err(e500)? };
    let body = html! {
        div.steps {
            div.step { span.n { "1" } "ValoLinux auf dem Gerät starten" }
            div.step { span.n { "2" } "Code vom Bildschirm eingeben" }
            div.step { span.n { "3" } "Name, Rolle und Gruppe wählen" }
        }
        @if let Some(d) = &found {
            section.card.adopt-card {
                div.found {
                    span.code-display { (format_code(&d.code)) }
                    div { b { (d.hostname) } br; span.muted.small { (d.hardware) " · " (d.os) " · gemeldet " (ago(Some(d.created))) } }
                }
                form method="post" action="/adopt" {
                    (csrf(&admin))
                    input type="hidden" name="code" value=(d.code);
                    label { "Name" input name="name" placeholder=(d.hostname) maxlength="80" autofocus; }
                    div.field { span.label { "Rolle" } (role_choice("student")) }
                    label { "Gruppe / Klasse" (group_select(&groups, None)) }
                    div.actions {
                        a.button href="/adopt" { "Abbrechen" }
                        button.primary type="submit" { "Gerät hinzufügen" }
                    }
                }
            }
        } @else {
            section.card.hero {
                div {
                    h2 { "Code eingeben" }
                    @if !code.is_empty() {
                        p.flash.err { "Kein wartendes Gerät mit dem Code " b { (format_code(&code)) } " gefunden." }
                    } @else {
                        p.muted { "Der Code steht auf dem Bildschirm des neuen Geräts." }
                    }
                }
                (code_form(&admin, true))
            }
        }
        section.card {
            h2 { "Wartende Geräte" }
            @if pending.is_empty() {
                p.empty { "Gerade wartet kein Gerät." }
            } @else {
                table.list {
                    thead { tr { th { "Code" } th { "Gerät" } th { "Gemeldet" } th {} } }
                    tbody {
                        @for d in &pending {
                            tr {
                                td { span.code-small { (format_code(&d.code)) } }
                                td { b { (d.hostname) } br; span.muted.small { (d.hardware) } }
                                td.muted { (ago(d.last_seen)) }
                                td.right { a.button href={"/adopt?code=" (d.code)} { "Hinzufügen" } }
                            }
                        }
                    }
                }
                p.muted.small { "Nur Geräte hinzufügen, deren Code du selbst auf dem Bildschirm gesehen hast." }
            }
        }
    };
    Ok(layout(&s, &admin, "/adopt", "Gerät hinzufügen", &q, body))
}

async fn adopt_submit(State(s): State<Shared>, admin: Admin, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    let code = util::normalize_code(&f.text("code"));
    let Some(d) = s.db.pending_by_code(&code).map_err(e500)? else {
        return Ok(redirect_err("/adopt", "Dieses Gerät wartet nicht mehr."));
    };
    let name: String = f.text("name").chars().take(80).collect();
    let pw = util::admin_password();
    s.db
        .adopt(d.id, &name, parse_role(&f), parse_group(&f), &pw, &util::crypt_sha512(&pw))
        .map_err(e500)?;
    Ok(redirect_msg(&format!("/devices/{}", d.id), "Gerät hinzugefügt. Es holt sich seine Einstellungen in Kürze."))
}

// ---------------------------------------------------------------------------
// groups
// ---------------------------------------------------------------------------

async fn groups_page(State(s): State<Shared>, admin: Admin, Query(q): Q) -> Page {
    let groups = s.db.groups().map_err(e500)?;
    let body = html! {
        p.muted { "Gruppen sind z. B. Klassen oder Zimmer. Jede Gruppe kann eigene Einstellungen haben." }
        div.tiles {
            @for g in &groups {
                div.tile {
                    h3 { (g.name) }
                    p.muted { (g.device_count) " Geräte" }
                    div.tile-actions {
                        a.button href={"/devices?group=" (g.id)} { "Geräte" }
                        a.button href={"/settings/group:" (g.id)} { "Einstellungen" }
                        form method="post" action={"/groups/" (g.id) "/delete"}
                            onsubmit="return confirm('Gruppe löschen? Die Geräte bleiben erhalten.')" {
                            (csrf(&admin)) button.link.danger-text type="submit" { "Löschen" }
                        }
                    }
                }
            }
            form.tile.new method="post" action="/groups" {
                (csrf(&admin))
                h3 { "Neue Gruppe" }
                input name="name" placeholder="z. B. 5a oder Zimmer 12" required maxlength="60";
                button.primary type="submit" { "Anlegen" }
            }
        }
    };
    Ok(layout(&s, &admin, "/groups", "Gruppen", &q, body))
}

async fn group_create(State(s): State<Shared>, admin: Admin, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    let name: String = f.text("name").chars().take(60).collect();
    if name.is_empty() {
        return Ok(redirect_err("/groups", "Name fehlt."));
    }
    match s.db.create_group(&name) {
        Ok(_) => Ok(redirect_msg("/groups", "Gruppe angelegt.")),
        Err(_) => Ok(redirect_err("/groups", "Eine Gruppe mit diesem Namen gibt es schon.")),
    }
}

async fn group_delete(State(s): State<Shared>, admin: Admin, Path(id): Path<i64>, body: String) -> Page {
    admin.check_csrf(&FormData::parse(&body))?;
    s.db.delete_group(id).map_err(e500)?;
    Ok(redirect_msg("/groups", "Gruppe gelöscht."))
}

// ---------------------------------------------------------------------------
// settings
// ---------------------------------------------------------------------------

/// The value a scope inherits if it does not set a field itself.
fn inherited(s: &Shared, scope: &Scope) -> Result<(Settings, &'static str), Response> {
    Ok(match scope {
        Scope::Global => (Settings::defaults(), "Standard"),
        Scope::Role(_) | Scope::Group(_) => {
            (Settings::resolve([&s.db.policy("global").map_err(e500)?]), "von Global")
        }
        Scope::Device(id) => {
            let d = s.db.device(*id).map_err(e500)?.ok_or_else(not_found)?;
            let layers = s.db.layers_for(&d).map_err(e500)?;
            let upper: Vec<&Settings> =
                layers.iter().filter(|(k, _)| !k.starts_with("device:")).map(|(_, l)| l).collect();
            (Settings::resolve(upper), "von Gruppe/Rolle")
        }
    })
}

fn field_input(f: &Field, current: Option<&Value>, parent: Option<&Value>, from: &str, catalog: &[CatalogEntry]) -> Markup {
    let parent_txt = fields::display(f, parent, catalog);
    let cur_str = current.and_then(|v| v.as_str()).unwrap_or("");
    html! {
        div.setting {
            div.setting-label {
                b { (f.label) }
                @if !f.help.is_empty() { span.muted.small { (f.help) } }
                @if current.is_none() { span.inherit-note { "erbt " (from) ": " (parent_txt) } }
                @else { span.override-note { "hier festgelegt" } }
            }
            div.setting-input {
                @match &f.kind {
                    Kind::Choice(opts) => {
                        select name=(f.key) {
                            option value="" selected[current.is_none()] { "Erben (" (parent_txt) ")" }
                            @for (k, l) in *opts { option value=(k) selected[cur_str == *k] { (l) } }
                        }
                    }
                    Kind::Text { placeholder } => {
                        input name=(f.key) value=(cur_str) placeholder={"Erben: " (parent_txt)} title=(placeholder);
                    }
                    Kind::Bool => {
                        @let b = current.and_then(|v| v.as_bool());
                        div.segmented {
                            label { input type="radio" name=(f.key) value="" checked[b.is_none()]; span { "Erben" } }
                            label { input type="radio" name=(f.key) value="true" checked[b == Some(true)]; span { "Ja" } }
                            label { input type="radio" name=(f.key) value="false" checked[b == Some(false)]; span { "Nein" } }
                        }
                    }
                    Kind::Apps => {
                        @let selected: Vec<&str> = current.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
                        @let parent_sel: Vec<&str> = parent.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
                        div.segmented {
                            label { input type="radio" name="apps_mode" value="inherit" checked[current.is_none()]; span { "Erben" } }
                            label { input type="radio" name="apps_mode" value="set" checked[current.is_some()]; span { "Eigene Auswahl" } }
                        }
                        div.appgrid {
                            @for c in catalog {
                                @let on = if current.is_some() { selected.contains(&c.app.id.as_str()) } else { parent_sel.contains(&c.app.id.as_str()) };
                                label.appcard {
                                    input type="checkbox" name="apps_item" value=(c.app.id) checked[on];
                                    span.appicon { (c.app.name.chars().next().unwrap_or('?')) }
                                    span { b { (c.app.name) } br; span.muted.small { (c.description) } }
                                }
                            }
                        }
                        p.muted.small { "Häkchen gelten nur mit „Eigene Auswahl“." }
                    }
                }
            }
        }
    }
}

async fn settings_page(State(s): State<Shared>, admin: Admin, Path(scope_s): Path<String>, Query(q): Q) -> Page {
    let scope = Scope::parse(&scope_s).ok_or_else(not_found)?;
    let groups = s.db.groups().map_err(e500)?;
    let catalog = s.db.catalog().map_err(e500)?;
    let current = fields::to_map(&s.db.policy(&scope.key()).map_err(e500)?);
    let (parent, from) = inherited(&s, &scope)?;
    let parent = fields::to_map(&parent);
    let title = match &scope {
        Scope::Device(id) => {
            let d = s.db.device(*id).map_err(e500)?.ok_or_else(not_found)?;
            format!("Einstellungen: {}", d.display_name())
        }
        _ => format!("Einstellungen: {}", scope_label(&s, &scope.key())),
    };
    let key = scope.key();
    let body = html! {
        @if !matches!(scope, Scope::Device(_)) {
            nav.tabs {
                a.active[key == "global"] href="/settings/global" { "Global" }
                a.active[key == "role:student"] href="/settings/role:student" { "Schüler/innen" }
                a.active[key == "role:teacher"] href="/settings/role:teacher" { "Lehrpersonen" }
                @for g in &groups {
                    @let k = format!("group:{}", g.id);
                    a.active[key == k] href={"/settings/" (k)} { (g.name) }
                }
            }
        } @else if let Scope::Device(id) = scope {
            p { a href={"/devices/" (id)} { "← zurück zum Gerät" } }
        }
        p.muted {
            @match scope {
                Scope::Global => "Gilt für alle Geräte, solange Rolle, Gruppe oder Gerät nichts anderes festlegen.",
                Scope::Role(_) => "Gilt für alle Geräte mit dieser Rolle. Gruppen und einzelne Geräte können das übersteuern.",
                Scope::Group(_) => "Gilt für alle Geräte in dieser Gruppe. Einzelne Geräte können das übersteuern.",
                Scope::Device(_) => "Gilt nur für dieses Gerät und hat Vorrang vor allem anderen.",
            }
        }
        form.settings method="post" {
            (csrf(&admin))
            @for sec in SECTIONS {
                section.card {
                    h2 { span.sec-icon { (sec.icon) } (sec.title) }
                    @for f in sec.fields {
                        (field_input(f, current.get(f.key), parent.get(f.key), from, &catalog))
                    }
                }
            }
            div.savebar {
                span.muted.small { "Geräte übernehmen Änderungen beim nächsten Abgleich (meist innert weniger Minuten)." }
                button.primary type="submit" { "Speichern" }
            }
        }
    };
    Ok(layout(&s, &admin, "/settings", &title, &q, body))
}

async fn settings_submit(State(s): State<Shared>, admin: Admin, Path(scope_s): Path<String>, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    let scope = Scope::parse(&scope_s).ok_or_else(not_found)?;
    let catalog = s.db.catalog().map_err(e500)?;
    s.db.set_policy(&scope.key(), &fields::parse_form(&f, &catalog)).map_err(e500)?;
    Ok(redirect_msg(&format!("/settings/{}", scope.key()), "Einstellungen gespeichert."))
}

// ---------------------------------------------------------------------------
// app catalog
// ---------------------------------------------------------------------------

async fn apps_page(State(s): State<Shared>, admin: Admin, Query(q): Q) -> Page {
    let catalog = s.db.catalog().map_err(e500)?;
    let body = html! {
        p.muted {
            "Nur Apps aus diesem Katalog können auf Geräten installiert werden (als Flatpak). "
            "Welche Apps ein Gerät bekommt, legst du unter " a href="/settings" { "Einstellungen" } " fest."
        }
        div.tiles {
            div.tile.fixed {
                span.appicon.big { "F" }
                h3 { "Firefox ESR" }
                p.muted.small { "Im Image enthalten" }
            }
            div.tile.fixed {
                span.appicon.big { "W" }
                h3 { "WordCraft · GridCraft · DeckCraft" }
                p.muted.small { "Im Image enthalten (ArtCraft, Vorabversion)" }
            }
            @for c in &catalog {
                div.tile {
                    span.appicon.big { (c.app.name.chars().next().unwrap_or('?')) }
                    h3 { (c.app.name) }
                    p.muted.small { (c.description) }
                    code.small { (c.app.flatpak_ref) }
                    div.tile-actions {
                        form method="post" action={"/apps/" (c.app.id) "/delete"}
                            onsubmit="return confirm('App aus dem Katalog entfernen? Sie wird beim nächsten Abgleich von den Geräten entfernt.')" {
                            (csrf(&admin)) button.link.danger-text type="submit" { "Entfernen" }
                        }
                    }
                }
            }
        }
        section.card {
            h2 { "App zum Katalog hinzufügen" }
            form.inline method="post" action="/apps" {
                (csrf(&admin))
                label { "Name" input name="name" required maxlength="60" placeholder="z. B. Scratch"; }
                label { "Flatpak-ID" input name="flatpak_ref" required maxlength="120" placeholder="z. B. edu.mit.Scratch"
                    pattern="[A-Za-z0-9_\\-]+(\\.[A-Za-z0-9_\\-]+)+"; }
                label { "Beschreibung" input name="description" maxlength="120"; }
                button.primary type="submit" { "Hinzufügen" }
            }
            p.muted.small { "Die Flatpak-ID findest du auf flathub.org in der Adresse der App." }
        }
    };
    Ok(layout(&s, &admin, "/apps", "App-Katalog", &q, body))
}

fn valid_flatpak_ref(r: &str) -> bool {
    let parts: Vec<&str> = r.split('.').collect();
    parts.len() >= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

async fn app_create(State(s): State<Shared>, admin: Admin, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    let name = f.text("name");
    let fref = f.text("flatpak_ref");
    if name.is_empty() || !valid_flatpak_ref(&fref) {
        return Ok(redirect_err("/apps", "Name fehlt oder Flatpak-ID ungültig (Form: org.beispiel.App)."));
    }
    let mut id = slug(&name);
    if id.is_empty() {
        id = slug(&fref);
    }
    match s.db.add_catalog(&id, &name, &fref, &f.text("description")) {
        Ok(()) => Ok(redirect_msg("/apps", "App hinzugefügt.")),
        Err(_) => Ok(redirect_err("/apps", "Diese App (oder ein gleicher Name) ist schon im Katalog.")),
    }
}

async fn app_delete(State(s): State<Shared>, admin: Admin, Path(id): Path<String>, body: String) -> Page {
    admin.check_csrf(&FormData::parse(&body))?;
    s.db.delete_catalog(&id).map_err(e500)?;
    Ok(redirect_msg("/apps", "App entfernt."))
}

#[cfg(test)]
mod tests {
    #[test]
    fn flatpak_ref_validation_and_slug() {
        assert!(super::valid_flatpak_ref("org.gimp.GIMP"));
        assert!(!super::valid_flatpak_ref("gimp"));
        assert!(!super::valid_flatpak_ref("org.gimp; rm -rf /"));
        assert_eq!(super::slug("Tux Paint!"), "tux-paint");
    }
}

