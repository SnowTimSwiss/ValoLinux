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
        .route("/login/2fa", post(login_2fa))
        .route("/account", get(account_page))
        .route("/account/password", post(account_password))
        .route("/account/2fa", post(account_2fa_enable))
        .route("/account/2fa/disable", post(account_2fa_disable))
        .route("/account/admins", post(admin_create))
        .route("/account/admins/{id}/delete", post(admin_delete))
        .route("/logout", post(logout))
        .route("/devices", get(devices_page))
        .route("/devices/bulk", post(devices_bulk))
        .route("/devices/{id}", get(device_page).post(device_update))
        .route("/devices/{id}/password", post(device_password))
        .route("/devices/{id}/delete", post(device_delete))
        .route("/devices/{id}/action", post(device_action))
        .route("/groups/{id}/action", post(group_action))
        .route("/adopt", get(adopt_page).post(adopt_submit))
        .route("/groups", get(groups_page).post(group_create))
        .route("/groups/{id}/delete", post(group_delete))
        .route("/settings", get(|| async { Redirect::to("/settings/global") }))
        .route("/settings/{scope}", get(settings_page).post(settings_submit))
        .route("/log", get(log_page))
        .route("/apps", get(apps_page).post(app_create))
        .route("/apps/{id}/delete", post(app_delete))
        .route(
            "/wallpapers",
            get(wallpapers_page)
                .post(wallpaper_upload)
                .layer(axum::extract::DefaultBodyLimit::max(MAX_UPLOAD + 64 * 1024)),
        )
        .route("/wallpapers/{sha}/delete", post(wallpaper_delete))
        .route("/assets/{sha}", get(asset))
        .route("/static/default-wallpaper.png", get(default_wallpaper))
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
        ("/wallpapers", "Hintergründe", "◪"),
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
                        a.account-link.active[active == "/account"] href="/account" { "Angemeldet als " b { (admin.username) } }
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

async fn login_page(State(s): State<Shared>, Query(q): Q) -> Response {
    if s.db.admin_count().unwrap_or(0) == 0 {
        return Redirect::to("/setup").into_response();
    }
    auth_page("Anmelden", "Verwaltung der Schulgeräte", "/login", "Anmelden", q.get("err").map(String::as_str), false)
}

async fn login_submit(State(s): State<Shared>, body: String) -> Page {
    let f = FormData::parse(&body);
    let user = f.text("username");
    let pw = f.get("password").unwrap_or("");
    if auth::locked_out(&s, &user) {
        tracing::warn!("login for {user:?} refused: locked after failed attempts");
        return Ok(auth_page("Anmelden", "Verwaltung der Schulgeräte", "/login", "Anmelden",
            Some("Zu viele Fehlversuche. Bitte 15 Minuten warten."), false));
    }
    if let Some(a) = s.db.admin_by_name(&user).map_err(e500)? {
        if auth::verify_password(pw, &a.password_hash) {
            auth::clear_failures(&s, &user);
            if a.totp_secret.is_some() {
                let token = util::random_token();
                s.db.create_pending_login(&token, a.id, valo_proto::now_unix() + 300).map_err(e500)?;
                return Ok(totp_page(&token, None));
            }
            let cookie = auth::login_cookie(&s, a.id).map_err(e500)?;
            return Ok(([(header::SET_COOKIE, cookie)], Redirect::to("/")).into_response());
        }
    } else {
        // spend comparable time for unknown users
        let _ = auth::verify_password(pw, "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    }
    auth::record_failure(&s, &user);
    tracing::warn!("failed login for {user:?}");
    Ok(auth_page("Anmelden", "Verwaltung der Schulgeräte", "/login", "Anmelden", Some("Benutzername oder Passwort falsch."), false))
}

fn totp_page(token: &str, err: Option<&str>) -> Response {
    html! {
        (head("Bestätigen"))
        body.auth {
            div.auth-card {
                img.auth-logo src="/static/logo.svg" alt="";
                h1 { b { "valo" } "linux" }
                p.muted { "Gib den 6-stelligen Code aus deiner Authenticator-App ein." }
                @if let Some(e) = err { div.flash.err { (e) } }
                form method="post" action="/login/2fa" {
                    input type="hidden" name="token" value=(token);
                    label { "Code" input.code name="code" inputmode="numeric" pattern="[0-9 ]{6,7}" maxlength="7"
                        autocomplete="one-time-code" required autofocus; }
                    button.primary.wide type="submit" { "Bestätigen" }
                }
            }
        }
    }
    .into_response()
}

async fn login_2fa(State(s): State<Shared>, body: String) -> Page {
    let f = FormData::parse(&body);
    // single-use token: a wrong code means starting over with the password
    let Some(id) = s.db.take_pending_login(&f.text("token")).map_err(e500)? else {
        return Ok(redirect_err("/login", "Anmeldung abgelaufen, bitte nochmals."));
    };
    let a = s.db.admin_by_id(id).map_err(e500)?.ok_or_else(not_found)?;
    let ok = a.totp_secret.as_deref().is_some_and(|sec| auth::check_totp(sec, &a.username, &f.text("code")));
    if !ok {
        auth::record_failure(&s, &a.username);
        tracing::warn!("wrong 2FA code for {:?}", a.username);
        return Ok(redirect_err("/login", "Code falsch. Bitte nochmals mit Passwort anmelden."));
    }
    let cookie = auth::login_cookie(&s, a.id).map_err(e500)?;
    Ok(([(header::SET_COOKIE, cookie)], Redirect::to("/")).into_response())
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
    let audit = s.db.audit(6).map_err(e500)?;
    let mut recent: Vec<Device> = adopted.iter().map(|d| (*d).clone()).collect();
    recent.sort_by_key(|d| std::cmp::Reverse(d.last_seen));
    let body = html! {
        @if !admin.has_2fa {
            div.flash.warn { "Dein Konto hat noch keine Zwei-Faktor-Anmeldung. " a href="/account" { "Jetzt einrichten" } }
        }
        div.stats {
            a.stat href="/devices" { span.num { (adopted.len()) } span.lbl { "Geräte" } span.sub { (teachers) (if teachers == 1 { " Lehrperson · " } else { " Lehrpersonen · " }) (adopted.len() - teachers) " Schüler/innen" } }
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
        @if !audit.is_empty() {
            section.card {
                div.card-head { h2 { "Letzte Änderungen" } a.button href="/log" { "Ganzes Protokoll" } }
                (audit_table(&audit))
            }
        }
        section.card {
            div.card-head { h2 { "Zuletzt gesehen" } a.button href="/devices" { "Alle Geräte" } }
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
    device_table_sel(devices, false)
}

fn device_table_sel(devices: &[Device], selectable: bool) -> Markup {
    html! {
        table.list {
            thead { tr {
                @if selectable {
                    th.check { input type="checkbox" aria-label="Alle auswählen"
                        onclick="document.querySelectorAll('input[name=ids]').forEach(c => c.checked = this.checked)"; }
                }
                th { "Gerät" } th { "Rolle" } th { "Gruppe" } th { "Status" } th { "Zuletzt gesehen" } } }
            tbody {
                @for d in devices {
                    tr {
                        @if selectable { td.check { input type="checkbox" name="ids" value=(d.id) form="bulk" aria-label="auswählen"; } }
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
            @if devices.is_empty() { p.empty { "Keine Geräte gefunden." } } @else { (device_table_sel(&devices, true)) }
        }
        @if !devices.is_empty() {
            form id="bulk" class="savebar" method="post" action="/devices/bulk" {
                (csrf(&admin))
                span.muted.small { "Ausgewählte Geräte:" }
                select name="role" aria-label="Rolle" {
                    option value="" { "Rolle unverändert" }
                    option value="student" { "→ Schüler/in" }
                    option value="teacher" { "→ Lehrperson" }
                }
                select name="group" aria-label="Gruppe" {
                    option value="" { "Gruppe unverändert" }
                    option value="none" { "→ keine Gruppe" }
                    @for g in &groups { option value=(g.id) { "→ " (g.name) } }
                }
                button.primary type="submit" { "Übernehmen" }
            }
        }
    };
    Ok(layout(&s, &admin, "/devices", "Geräte", &q, body))
}

async fn devices_bulk(State(s): State<Shared>, admin: Admin, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    let ids: Vec<i64> = f.all("ids").iter().filter_map(|v| v.parse().ok()).collect();
    if ids.is_empty() {
        return Ok(redirect_err("/devices", "Keine Geräte ausgewählt."));
    }
    let role = match f.text("role").as_str() {
        "student" => Some("student"),
        "teacher" => Some("teacher"),
        _ => None,
    };
    let group: Option<Option<i64>> = match f.text("group").as_str() {
        "" => None,
        "none" => Some(None),
        g => Some(Some(g.parse().map_err(|_| redirect_err("/devices", "Ungültige Gruppe."))?)),
    };
    let mut n = 0;
    for id in ids {
        if let Some(d) = s.db.device(id).map_err(e500)?.filter(|d| d.adopted()) {
            let r = role.unwrap_or(&d.role);
            let g = group.unwrap_or(d.group_id);
            s.db.update_device(d.id, &d.name, r, g).map_err(e500)?;
            n += 1;
        }
    }
    Ok(redirect_msg("/devices", &format!("{n} Geräte aktualisiert.")))
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
    let actions = s.db.recent_actions(d.id).map_err(e500)?;

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
                h2 { "Aktionen" }
                p.muted { "Das Gerät führt die Aktion beim nächsten Abgleich aus (innert ca. 1 Minute, wenn es eingeschaltet ist)." }
                div.action-row {
                    @for (kind, label) in ACTIONS {
                        form method="post" action={"/devices/" (d.id) "/action"}
                            onsubmit={"return confirm('" (label) "?')"} {
                            (csrf(&admin)) input type="hidden" name="kind" value=(kind);
                            button type="submit" { (label) }
                        }
                    }
                }
                @if !actions.is_empty() {
                    table.list.compact {
                        tbody {
                            @for (kind, created, done) in &actions {
                                tr {
                                    td { (action_label(kind)) }
                                    td.muted { (ago(Some(*created))) }
                                    td { @match done {
                                        Some(t) => span.pill.good { span.dot {} "ausgeführt " (ago(Some(*t))) },
                                        None => span.pill.wait { span.dot {} "wartet" },
                                    } }
                                }
                            }
                        }
                    }
                    @if actions.iter().any(|a| a.2.is_none()) {
                        form method="post" action={"/devices/" (d.id) "/action"} {
                            (csrf(&admin)) input type="hidden" name="kind" value="cancel";
                            button.link type="submit" { "Wartende Aktionen abbrechen" }
                        }
                    }
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

const ACTIONS: &[(&str, &str)] = &[("reboot", "Neu starten"), ("poweroff", "Ausschalten")];

fn action_label(kind: &str) -> &str {
    ACTIONS.iter().find(|(k, _)| *k == kind).map(|(_, l)| *l).unwrap_or(kind)
}

async fn device_action(State(s): State<Shared>, admin: Admin, Path(id): Path<i64>, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    s.db.device(id).map_err(e500)?.ok_or_else(not_found)?;
    let kind = f.text("kind");
    if kind == "cancel" {
        s.db.cancel_actions(id).map_err(e500)?;
        return Ok(redirect_msg(&format!("/devices/{id}"), "Wartende Aktionen abgebrochen."));
    }
    if !ACTIONS.iter().any(|(k, _)| *k == kind) {
        return Ok(redirect_err(&format!("/devices/{id}"), "Unbekannte Aktion."));
    }
    s.db.queue_action(id, &kind).map_err(e500)?;
    Ok(redirect_msg(&format!("/devices/{id}"), &format!("{}: wird beim nächsten Abgleich ausgeführt.", action_label(&kind))))
}

async fn group_action(State(s): State<Shared>, admin: Admin, Path(id): Path<i64>, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    let kind = f.text("kind");
    if !ACTIONS.iter().any(|(k, _)| *k == kind) {
        return Ok(redirect_err("/groups", "Unbekannte Aktion."));
    }
    let devices: Vec<Device> =
        s.db.devices().map_err(e500)?.into_iter().filter(|d| d.adopted() && d.group_id == Some(id)).collect();
    for d in &devices {
        s.db.queue_action(d.id, &kind).map_err(e500)?;
    }
    Ok(redirect_msg("/groups", &format!("{}: an {} Geräte gesendet.", action_label(&kind), devices.len())))
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
                    p.muted { (g.device_count) (if g.device_count == 1 { " Gerät" } else { " Geräte" }) }
                    div.tile-actions {
                        a.button href={"/devices?group=" (g.id)} { "Geräte" }
                        a.button href={"/settings/group:" (g.id)} { "Einstellungen" }
                    }
                    div.tile-actions {
                        @for (kind, label) in ACTIONS {
                            form method="post" action={"/groups/" (g.id) "/action"}
                                onsubmit={"return confirm('Alle Geräte in " (g.name) ": " (label) "?')"} {
                                (csrf(&admin)) input type="hidden" name="kind" value=(kind);
                                button.small-btn type="submit" { "Alle: " (label) }
                            }
                        }
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

type Assets = [(String, String, String)];

fn field_input(
    f: &Field,
    current: Option<&Value>,
    parent: Option<&Value>,
    from: &str,
    catalog: &[CatalogEntry],
    assets: &Assets,
) -> Markup {
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
                    Kind::Lines { placeholder } => {
                        @let text = current.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
                        textarea name=(f.key) rows="4" placeholder={"Leer = erben (" (parent_txt) ")\n" (placeholder)} { (text) }
                    }
                    Kind::Bool => {
                        @let b = current.and_then(|v| v.as_bool());
                        div.segmented {
                            label { input type="radio" name=(f.key) value="" checked[b.is_none()]; span { "Erben" } }
                            label { input type="radio" name=(f.key) value="true" checked[b == Some(true)]; span { "Ja" } }
                            label { input type="radio" name=(f.key) value="false" checked[b == Some(false)]; span { "Nein" } }
                        }
                    }
                    Kind::Wallpaper => {
                        div.wallgrid {
                            label.wallcard {
                                input type="radio" name=(f.key) value="" checked[current.is_none()];
                                span.wallthumb.inherit { "Erben" }
                                span.small { (from) ": " (parent_txt) }
                            }
                            label.wallcard {
                                input type="radio" name=(f.key) value="default" checked[cur_str == "default"];
                                img.wallthumb src="/static/default-wallpaper.png" alt="" loading="lazy";
                                span.small { "ValoLinux" }
                            }
                            @for (sha, name, _) in assets {
                                label.wallcard {
                                    input type="radio" name=(f.key) value=(sha) checked[cur_str == sha.as_str()];
                                    img.wallthumb src={"/assets/" (sha)} alt="" loading="lazy";
                                    span.small { (name) }
                                }
                            }
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
    let assets = s.db.assets().map_err(e500)?;
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
                        (field_input(f, current.get(f.key), parent.get(f.key), from, &catalog, &assets))
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



// ---------------------------------------------------------------------------
// account: password, 2FA, other admins
// ---------------------------------------------------------------------------

async fn account_page(State(s): State<Shared>, admin: Admin, Query(q): Q) -> Page {
    let admins = s.db.admins().map_err(e500)?;
    let setup = if admin.has_2fa {
        None
    } else {
        let secret = auth::new_totp_secret();
        let totp = auth::totp(&secret, &admin.username).ok_or_else(|| e500("totp"))?;
        let qr = totp.to_qr_base64().map_err(e500)?;
        Some((secret, qr))
    };
    let body = html! {
        div.grid2 {
            section.card {
                h2 { "Zwei-Faktor-Anmeldung" }
                @if let Some((secret, qr)) = &setup {
                    p.muted { "Scanne den QR-Code mit einer Authenticator-App (z. B. Aegis, Google Authenticator, 2FAS) und gib den angezeigten Code ein." }
                    div.qr { img src={"data:image/png;base64," (qr)} alt="QR-Code" width="200" height="200"; }
                    details { summary.small { "Code manuell eingeben" } code.small { (secret) } }
                    form method="post" action="/account/2fa" {
                        (csrf(&admin))
                        input type="hidden" name="secret" value=(secret);
                        label { "Code aus der App" input.code name="code" inputmode="numeric" maxlength="7" required autocomplete="one-time-code"; }
                        button.primary type="submit" { "Aktivieren" }
                    }
                } @else {
                    p { span.pill.good { span.dot {} "aktiv" } }
                    p.muted { "Zum Deaktivieren einen aktuellen Code eingeben." }
                    form method="post" action="/account/2fa/disable" {
                        (csrf(&admin))
                        label { "Code aus der App" input.code name="code" inputmode="numeric" maxlength="7" required; }
                        button type="submit" { "Deaktivieren" }
                    }
                }
            }
            section.card {
                h2 { "Passwort ändern" }
                form method="post" action="/account/password" {
                    (csrf(&admin))
                    label { "Aktuelles Passwort" input type="password" name="current" required autocomplete="current-password"; }
                    label { "Neues Passwort" input type="password" name="password" required minlength="10" autocomplete="new-password"; }
                    label { "Wiederholen" input type="password" name="password2" required minlength="10" autocomplete="new-password"; }
                    button.primary type="submit" { "Ändern" }
                }
            }
        }
        section.card {
            h2 { "Admins" }
            table.list {
                thead { tr { th { "Benutzer" } th { "2FA" } th {} } }
                tbody {
                    @for a in &admins {
                        tr {
                            td { b { (a.username) } @if a.id == admin.id { span.muted { " (du)" } } }
                            td { @if a.totp_secret.is_some() { span.pill.good { span.dot {} "aktiv" } } @else { span.pill.wait { span.dot {} "fehlt" } } }
                            td.right {
                                @if a.id != admin.id {
                                    form method="post" action={"/account/admins/" (a.id) "/delete"}
                                        onsubmit="return confirm('Admin wirklich löschen?')" {
                                        (csrf(&admin)) button.link.danger-text type="submit" { "Löschen" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            h3.mt { "Neuen Admin anlegen" }
            form.inline.two method="post" action="/account/admins" {
                (csrf(&admin))
                label { "Benutzername" input name="username" required maxlength="40"; }
                label { "Startpasswort" input type="password" name="password" required minlength="10" autocomplete="new-password"; }
                button.primary type="submit" { "Anlegen" }
            }
            p.muted.small { "Alle Admins haben die gleichen Rechte. Jeder Admin sollte sofort sein Passwort ändern und 2FA einrichten." }
        }
    };
    Ok(layout(&s, &admin, "/account", "Konto", &q, body))
}

async fn account_password(State(s): State<Shared>, admin: Admin, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    let a = s.db.admin_by_id(admin.id).map_err(e500)?.ok_or_else(not_found)?;
    if !auth::verify_password(f.get("current").unwrap_or(""), &a.password_hash) {
        return Ok(redirect_err("/account", "Aktuelles Passwort ist falsch."));
    }
    let pw = f.get("password").unwrap_or("");
    if pw.chars().count() < 10 || Some(pw) != f.get("password2") {
        return Ok(redirect_err("/account", "Neues Passwort zu kurz (mind. 10 Zeichen) oder nicht gleich wiederholt."));
    }
    s.db.set_admin_login_password(admin.id, &auth::hash_password(pw)).map_err(e500)?;
    s.db.delete_sessions_of(admin.id, &admin.session_token).map_err(e500)?;
    Ok(redirect_msg("/account", "Passwort geändert. Andere Sitzungen wurden abgemeldet."))
}

async fn account_2fa_enable(State(s): State<Shared>, admin: Admin, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    let secret = f.text("secret");
    if !auth::check_totp(&secret, &admin.username, &f.text("code")) {
        return Ok(redirect_err("/account", "Code stimmt nicht. Uhrzeit auf dem Handy prüfen und neu scannen."));
    }
    s.db.set_admin_totp(admin.id, Some(&secret)).map_err(e500)?;
    Ok(redirect_msg("/account", "Zwei-Faktor-Anmeldung ist aktiv."))
}

async fn account_2fa_disable(State(s): State<Shared>, admin: Admin, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    let a = s.db.admin_by_id(admin.id).map_err(e500)?.ok_or_else(not_found)?;
    let ok = a.totp_secret.as_deref().is_some_and(|sec| auth::check_totp(sec, &a.username, &f.text("code")));
    if !ok {
        return Ok(redirect_err("/account", "Code falsch."));
    }
    s.db.set_admin_totp(admin.id, None).map_err(e500)?;
    Ok(redirect_msg("/account", "Zwei-Faktor-Anmeldung deaktiviert."))
}

async fn admin_create(State(s): State<Shared>, admin: Admin, body: String) -> Page {
    let f = FormData::parse(&body);
    admin.check_csrf(&f)?;
    let user: String = f.text("username").chars().take(40).collect();
    let pw = f.get("password").unwrap_or("");
    if user.is_empty() || pw.chars().count() < 10 {
        return Ok(redirect_err("/account", "Benutzername fehlt oder Passwort kürzer als 10 Zeichen."));
    }
    match s.db.create_admin(&user, &auth::hash_password(pw)) {
        Ok(_) => Ok(redirect_msg("/account", "Admin angelegt.")),
        Err(_) => Ok(redirect_err("/account", "Diesen Benutzernamen gibt es schon.")),
    }
}

async fn admin_delete(State(s): State<Shared>, admin: Admin, Path(id): Path<i64>, body: String) -> Page {
    admin.check_csrf(&FormData::parse(&body))?;
    if id == admin.id {
        return Ok(redirect_err("/account", "Du kannst dich nicht selbst löschen."));
    }
    s.db.delete_admin(id).map_err(e500)?;
    Ok(redirect_msg("/account", "Admin gelöscht."))
}

// ---------------------------------------------------------------------------
// wallpapers
// ---------------------------------------------------------------------------

const MAX_UPLOAD: usize = 15 * 1024 * 1024;

async fn default_wallpaper() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=86400")],
        &include_bytes!("../../image/config/includes.chroot/usr/share/backgrounds/valolinux.png")[..],
    )
}

/// Uploaded files are public on purpose: devices fetch them without a session.
/// The signed manifest carries the sha256, so devices verify what they download.
async fn asset(State(s): State<Shared>, Path(sha): Path<String>) -> Response {
    if !valo_proto::is_sha256(&sha) {
        return not_found();
    }
    let Ok(Some(kind)) = s.db.asset_kind(&sha) else { return not_found() };
    match tokio::fs::read(s.assets_dir.join(&sha)).await {
        Ok(data) => (
            [
                (header::CONTENT_TYPE, if kind == "png" { "image/png" } else { "image/jpeg" }),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            data,
        )
            .into_response(),
        Err(_) => not_found(),
    }
}

async fn wallpapers_page(State(s): State<Shared>, admin: Admin, Query(q): Q) -> Page {
    let assets = s.db.assets().map_err(e500)?;
    let body = html! {
        p.muted { "Lade eigene Hintergrundbilder hoch (PNG oder JPEG, ideal 1920×1080, max. 15 MB). "
            "Welches Bild ein Gerät bekommt, wählst du unter " a href="/settings" { "Einstellungen → Desktop" } "." }
        div.tiles {
            div.tile.fixed {
                img.wallthumb.big src="/static/default-wallpaper.png" alt="";
                h3 { "ValoLinux" } p.muted.small { "Standard, im Image enthalten" }
            }
            @for (sha, name, _) in &assets {
                div.tile {
                    img.wallthumb.big src={"/assets/" (sha)} alt="";
                    h3 { (name) }
                    div.tile-actions {
                        form method="post" action={"/wallpapers/" (sha) "/delete"}
                            onsubmit="return confirm('Bild löschen? Geräte, die es verwenden, behalten es bis zur nächsten Änderung.')" {
                            (csrf(&admin)) button.link.danger-text type="submit" { "Löschen" }
                        }
                    }
                }
            }
            form.tile.new method="post" action="/wallpapers" enctype="multipart/form-data" {
                (csrf(&admin))
                h3 { "Bild hochladen" }
                input type="text" name="name" placeholder="Name, z. B. Schulhaus" maxlength="60";
                input type="file" name="file" accept="image/png,image/jpeg" required;
                button.primary type="submit" { "Hochladen" }
            }
        }
    };
    Ok(layout(&s, &admin, "/wallpapers", "Hintergründe", &q, body))
}

async fn wallpaper_upload(State(s): State<Shared>, admin: Admin, mut mp: axum::extract::Multipart) -> Page {
    let (mut csrf_ok, mut name, mut data) = (false, String::new(), None::<Vec<u8>>);
    while let Ok(Some(field)) = mp.next_field().await {
        match field.name().unwrap_or("") {
            "csrf" => csrf_ok = field.text().await.ok().as_deref() == Some(admin.csrf.as_str()),
            "name" => name = field.text().await.unwrap_or_default().trim().chars().take(60).collect(),
            "file" => {
                let fname = field.file_name().unwrap_or("").to_string();
                let bytes = field.bytes().await.map_err(|_| redirect_err("/wallpapers", "Upload abgebrochen oder zu gross (max. 15 MB)."))?;
                if name.is_empty() {
                    name = fname.rsplit_once('.').map(|(n, _)| n).unwrap_or(&fname).chars().take(60).collect();
                }
                data = Some(bytes.to_vec());
            }
            _ => {}
        }
    }
    if !csrf_ok {
        return Err((StatusCode::FORBIDDEN, "Ungültiges Formular (CSRF). Bitte Seite neu laden.").into_response());
    }
    let Some(data) = data.filter(|d| !d.is_empty() && d.len() <= MAX_UPLOAD) else {
        return Ok(redirect_err("/wallpapers", "Keine Datei oder Datei zu gross."));
    };
    let Some(kind) = valo_proto::image_kind(&data) else {
        return Ok(redirect_err("/wallpapers", "Nur PNG- oder JPEG-Bilder."));
    };
    let sha = valo_proto::sha256_hex(&data);
    tokio::fs::write(s.assets_dir.join(&sha), &data).await.map_err(e500)?;
    let name = if name.is_empty() { "Bild".to_string() } else { name };
    s.db.add_asset(&sha, &name, kind, data.len() as i64).map_err(e500)?;
    Ok(redirect_msg("/wallpapers", "Bild hochgeladen. Jetzt unter Einstellungen → Desktop auswählen."))
}

async fn wallpaper_delete(State(s): State<Shared>, admin: Admin, Path(sha): Path<String>, body: String) -> Page {
    admin.check_csrf(&FormData::parse(&body))?;
    if !valo_proto::is_sha256(&sha) {
        return Err(not_found());
    }
    s.db.delete_asset(&sha).map_err(e500)?;
    let _ = tokio::fs::remove_file(s.assets_dir.join(&sha)).await;
    Ok(redirect_msg("/wallpapers", "Bild gelöscht."))
}

// ---------------------------------------------------------------------------
// audit log
// ---------------------------------------------------------------------------

/// Human-readable subject for a UI path ("/devices/5/action" -> "Gerät 5a-PC-01").
fn subject_of(s: &Shared, path: &str) -> (String, String) {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    match parts.as_slice() {
        ["devices", id, ..] if id.parse::<i64>().is_ok() => {
            let id: i64 = id.parse().unwrap_or(0);
            match s.db.device(id).ok().flatten() {
                Some(d) => (format!("Gerät {}", d.display_name()), format!("/devices/{id}")),
                None => ("Gerät".into(), "/devices".into()),
            }
        }
        ["groups", id, ..] if id.parse::<i64>().is_ok() => {
            let id: i64 = id.parse().unwrap_or(0);
            match s.db.group(id).ok().flatten() {
                Some(g) => (format!("Gruppe {}", g.name), "/groups".into()),
                None => ("Gruppe".into(), "/groups".into()),
            }
        }
        ["settings", scope] => {
            let scope = scope.replace("%3A", ":");
            (format!("Einstellungen {}", scope_label(s, &scope)), format!("/settings/{scope}"))
        }
        ["devices", ..] => ("Geräte".into(), "/devices".into()),
        ["groups", ..] => ("Gruppen".into(), "/groups".into()),
        ["apps", ..] => ("App-Katalog".into(), "/apps".into()),
        ["wallpapers", ..] => ("Hintergründe".into(), "/wallpapers".into()),
        ["account", ..] => ("Konto".into(), "/account".into()),
        ["adopt", ..] => ("Gerät hinzufügen".into(), "/adopt".into()),
        _ => (path.to_string(), String::new()),
    }
}

/// Records every successful admin change. Successful form posts redirect with
/// `?msg=...`; that message is what gets logged, with admin, time and subject.
pub async fn audit_layer(
    State(s): State<Shared>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if req.method() != axum::http::Method::POST {
        return next.run(req).await;
    }
    let path = req.uri().path().to_string();
    let admin = auth::session_token(req.headers())
        .and_then(|t| s.db.session(&t).ok().flatten())
        .and_then(|(id, _)| s.db.admin_by_id(id).ok().flatten())
        .map(|a| a.username);
    // subject before the change (a deleted device can't be looked up afterwards)
    let before = subject_of(&s, &path);
    let resp = next.run(req).await;
    let Some(admin) = admin else { return resp };
    let location = resp.headers().get(header::LOCATION).and_then(|v| v.to_str().ok()).unwrap_or("");
    if let Some((loc_path, query)) = location.split_once('?') {
        let msg = form_urlencoded::parse(query.as_bytes()).find(|(k, _)| k == "msg").map(|(_, v)| v.to_string());
        if let Some(msg) = msg {
            // prefer the redirect target when it names an object (e.g. after adopting)
            let (subject, link) = if loc_path.starts_with("/devices/") { subject_of(&s, loc_path) } else { before };
            if let Err(e) = s.db.add_audit(&admin, &subject, &link, &msg) {
                tracing::error!("audit log: {e}");
            }
        }
    }
    resp
}

fn audit_table(entries: &[crate::db::AuditEntry]) -> Markup {
    html! {
        table.list.compact {
            tbody {
                @for (ts, admin, subject, link, text) in entries {
                    tr {
                        td.muted.nowrap { (ago(Some(*ts))) }
                        td { b { (admin) } }
                        td { @if link.is_empty() { (subject) } @else { a href=(link) { (subject) } } }
                        td.muted { (text) }
                    }
                }
            }
        }
    }
}

async fn log_page(State(s): State<Shared>, admin: Admin, Query(q): Q) -> Page {
    let entries = s.db.audit(500).map_err(e500)?;
    let body = html! {
        p.muted { "Alle Änderungen durch Admins, neueste zuerst (die letzten 5000 werden aufbewahrt)." }
        section.card {
            @if entries.is_empty() { p.empty { "Noch keine Änderungen." } } @else { (audit_table(&entries)) }
        }
    };
    Ok(layout(&s, &admin, "/log", "Protokoll", &q, body))
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
