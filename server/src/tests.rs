//! End-to-end tests of the server, in-process (no network).

use crate::{AppState, app, auth, db::Db};
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;
use valo_proto::*;

fn state() -> crate::Shared {
    Arc::new(AppState { db: Db::open_in_memory().unwrap(), key: Identity::generate().unwrap(), secure_cookies: false })
}

async fn send(s: &crate::Shared, req: Request<Body>) -> (StatusCode, Vec<(String, String)>, String) {
    let resp = app(s.clone()).oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8_lossy(&body).to_string())
}

fn signed(device: &Identity, method: &str, path: &str, body: Vec<u8>) -> Request<Body> {
    let t = now_unix();
    let sig = device.sign(&request_message(method, path, t, &body));
    Request::builder()
        .method(method)
        .uri(path)
        .header(AGENT_HEADER_KEY, device.public_key())
        .header(AGENT_HEADER_TIME, t.to_string())
        .header(AGENT_HEADER_SIG, sig)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn form(path: &str, cookie: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn get(path: &str, cookie: &str) -> Request<Body> {
    Request::builder().uri(path).header(header::COOKIE, cookie).body(Body::empty()).unwrap()
}

/// Create the admin through /setup and return (cookie, csrf).
async fn login(s: &crate::Shared) -> (String, String) {
    let req = Request::builder()
        .method("POST")
        .uri("/setup")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("username=admin&password=correct-horse-1&password2=correct-horse-1"))
        .unwrap();
    let (st, headers, _) = send(s, req).await;
    assert_eq!(st, StatusCode::SEE_OTHER);
    let set = headers.iter().find(|(k, _)| k == "set-cookie").expect("session cookie").1.clone();
    let cookie = set.split(';').next().unwrap().to_string();
    let token = cookie.split_once('=').unwrap().1;
    let csrf = s.db.session(token).unwrap().unwrap().1;
    (cookie, csrf)
}

async fn manifest(s: &crate::Shared, dev: &Identity) -> ManifestResponse {
    let (st, _, body) = send(s, signed(dev, "GET", "/api/v1/manifest", vec![])).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    serde_json::from_str(&body).unwrap()
}

#[tokio::test]
async fn ui_requires_setup_then_login() {
    let s = state();
    let (st, h, _) = send(&s, get("/", "")).await;
    assert_eq!(st, StatusCode::SEE_OTHER);
    assert!(h.iter().any(|(k, v)| k == "location" && v == "/setup"));

    let (cookie, _) = login(&s).await;
    let (st, h, _) = send(&s, get("/", "")).await;
    assert_eq!(st, StatusCode::SEE_OTHER);
    assert!(h.iter().any(|(k, v)| k == "location" && v == "/login"));
    let (st, _, body) = send(&s, get("/", &cookie)).await;
    assert_eq!(st, StatusCode::OK);
    assert!(body.contains("Neues Gerät hinzufügen"));

    // setup can't be used a second time
    let req = Request::builder()
        .method("POST")
        .uri("/setup")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("username=evil&password=0123456789&password2=0123456789"))
        .unwrap();
    send(&s, req).await;
    assert_eq!(s.db.admin_count().unwrap(), 1);
}

#[tokio::test]
async fn login_checks_password() {
    let s = state();
    login(&s).await;
    let bad = Request::builder()
        .method("POST")
        .uri("/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("username=admin&password=wrong-password"))
        .unwrap();
    let (st, h, body) = send(&s, bad).await;
    assert_eq!(st, StatusCode::OK);
    assert!(body.contains("falsch"));
    assert!(!h.iter().any(|(k, _)| k == "set-cookie"));
    assert!(auth::verify_password("x", &auth::hash_password("x")));
}

#[tokio::test]
async fn full_adoption_flow() {
    let s = state();
    let (cookie, csrf) = login(&s).await;
    let dev = Identity::generate().unwrap();

    // enroll
    let req = EnrollRequest {
        public_key: dev.public_key(),
        hostname: "valo-test".into(),
        os: "Debian 13".into(),
        agent_version: "0.1.0".into(),
        hardware: "QEMU".into(),
    };
    let (st, _, body) = send(&s, signed(&dev, "POST", "/api/v1/enroll", serde_json::to_vec(&req).unwrap())).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let enrolled: EnrollResponse = serde_json::from_str(&body).unwrap();
    assert_eq!(enrolled.code.len(), 6);
    assert_eq!(enrolled.server_key, s.key.public_key());

    // enrolling again keeps the same code
    let (_, _, body) = send(&s, signed(&dev, "POST", "/api/v1/enroll", serde_json::to_vec(&req).unwrap())).await;
    assert_eq!(serde_json::from_str::<EnrollResponse>(&body).unwrap().code, enrolled.code);

    // pending until adopted
    match manifest(&s, &dev).await {
        ManifestResponse::Pending { code } => assert_eq!(code, enrolled.code),
        other => panic!("expected pending, got {other:?}"),
    }

    // adopt page finds the device by code (formatted with dash, lowercase)
    let code_typed = format!("{}-{}", &enrolled.code[..3], &enrolled.code[3..]).to_lowercase();
    let (st, _, body) = send(&s, get(&format!("/adopt?code={code_typed}"), &cookie)).await;
    assert_eq!(st, StatusCode::OK);
    assert!(body.contains("valo-test"));

    // CSRF is enforced
    let (st, _, _) = send(&s, form("/adopt", &cookie, &format!("code={}&name=PC&role=teacher", enrolled.code))).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    s.db.create_group("5a").unwrap();
    let gid = s.db.groups().unwrap()[0].id;
    let (st, _, _) = send(
        &s,
        form("/adopt", &cookie, &format!("csrf={csrf}&code={}&name=Lehrer-PC&role=teacher&group={gid}", enrolled.code)),
    )
    .await;
    assert_eq!(st, StatusCode::SEE_OTHER);

    // manifest is signed with the pinned key and carries the role
    let ManifestResponse::Adopted { manifest: signed_m } = manifest(&s, &dev).await else { panic!("not adopted") };
    let m = signed_m.open(&enrolled.server_key).unwrap();
    assert_eq!(m.device_name, "Lehrer-PC");
    assert_eq!(m.role, "teacher");
    assert_eq!(m.group.as_deref(), Some("5a"));
    assert!(m.admin_password_hash.as_deref().unwrap().starts_with("$6$"));
    assert_eq!(m.settings.keyboard.as_deref(), Some("ch"));
    assert!(m.apps.is_empty());

    // admin changes settings for teachers: keyboard + apps -> new revision
    let (st, _, _) = send(
        &s,
        form(
            "/settings/role:teacher",
            &cookie,
            &format!("csrf={csrf}&keyboard=ch%3Afr&allow_app_install=true&apps_mode=set&apps_item=gimp&apps_item=nonexistent"),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::SEE_OTHER);
    let ManifestResponse::Adopted { manifest: signed2 } = manifest(&s, &dev).await else { panic!() };
    let m2 = signed2.open(&enrolled.server_key).unwrap();
    assert_ne!(m.revision, m2.revision);
    assert_eq!(m2.settings.keyboard.as_deref(), Some("ch:fr"));
    assert_eq!(m2.settings.allow_app_install, Some(true));
    assert_eq!(m2.apps.len(), 1, "unknown catalog ids are dropped");
    assert_eq!(m2.apps[0].flatpak_ref, "org.gimp.GIMP");

    // device-level override beats the role
    let did = s.db.device_by_key(&dev.public_key()).unwrap().unwrap().id;
    send(&s, form(&format!("/settings/device:{did}"), &cookie, &format!("csrf={csrf}&keyboard=de&apps_mode=inherit"))).await;
    let ManifestResponse::Adopted { manifest: signed3 } = manifest(&s, &dev).await else { panic!() };
    assert_eq!(signed3.open(&enrolled.server_key).unwrap().settings.keyboard.as_deref(), Some("de"));

    // report
    let report = Report {
        revision: Some(m2.revision.clone()),
        ok: false,
        messages: vec!["flatpak: no network".into()],
        agent_version: "0.1.0".into(),
        hostname: "valo-test".into(),
        os: String::new(),
        done_actions: vec![],
    };
    let (st, _, _) = send(&s, signed(&dev, "POST", "/api/v1/report", serde_json::to_vec(&report).unwrap())).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let d = s.db.device(did).unwrap().unwrap();
    assert_eq!(d.last_ok, Some(false));
    assert_eq!(d.os, "Debian 13", "empty os in report keeps the old value");

    // device page renders, shows the error and the password
    let (st, _, body) = send(&s, get(&format!("/devices/{did}"), &cookie)).await;
    assert_eq!(st, StatusCode::OK);
    assert!(body.contains("flatpak: no network"));
    assert!(body.contains(d.admin_password.as_deref().unwrap()));
    for page in ["/devices", "/groups", "/apps", "/settings/global", "/settings/role:student", &format!("/settings/group:{gid}")] {
        let (st, _, _) = send(&s, get(page, &cookie)).await;
        assert_eq!(st, StatusCode::OK, "{page}");
    }
}

#[tokio::test]
async fn bad_signatures_are_rejected() {
    let s = state();
    let dev = Identity::generate().unwrap();
    let other = Identity::generate().unwrap();

    // missing headers
    let req = Request::builder().uri("/api/v1/manifest").body(Body::empty()).unwrap();
    assert_eq!(send(&s, req).await.0, StatusCode::UNAUTHORIZED);

    // signature from another key
    let mut req = signed(&other, "GET", "/api/v1/manifest", vec![]);
    req.headers_mut().insert(AGENT_HEADER_KEY, dev.public_key().parse().unwrap());
    assert_eq!(send(&s, req).await.0, StatusCode::UNAUTHORIZED);

    // signature for a different path
    let mut req = signed(&dev, "GET", "/api/v1/other", vec![]);
    *req.uri_mut() = "/api/v1/manifest".parse().unwrap();
    assert_eq!(send(&s, req).await.0, StatusCode::UNAUTHORIZED);

    // enrolling someone else's key
    let req = EnrollRequest {
        public_key: other.public_key(),
        hostname: "x".into(),
        os: String::new(),
        agent_version: String::new(),
        hardware: String::new(),
    };
    let (st, _, _) = send(&s, signed(&dev, "POST", "/api/v1/enroll", serde_json::to_vec(&req).unwrap())).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // unknown device asking for a manifest
    assert_eq!(send(&s, signed(&dev, "GET", "/api/v1/manifest", vec![])).await.0, StatusCode::NOT_FOUND);
}

fn login_form(user: &str, pw: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(format!("username={user}&password={pw}")))
        .unwrap()
}

fn hidden_token(html: &str) -> String {
    let i = html.find("name=\"token\" value=\"").expect("token field") + 20;
    html[i..i + 64].to_string()
}

#[tokio::test]
async fn two_factor_login() {
    let s = state();
    let (cookie, csrf) = login(&s).await;

    // wrong code does not enable 2FA
    let secret = auth::new_totp_secret();
    send(&s, form("/account/2fa", &cookie, &format!("csrf={csrf}&secret={secret}&code=000000"))).await;
    assert!(s.db.admin_by_name("admin").unwrap().unwrap().totp_secret.is_none());

    let code = auth::totp(&secret, "admin").unwrap().generate_current().to_string();
    let (st, _, _) = send(&s, form("/account/2fa", &cookie, &format!("csrf={csrf}&secret={secret}&code={code}"))).await;
    assert_eq!(st, StatusCode::SEE_OTHER);
    assert_eq!(s.db.admin_by_name("admin").unwrap().unwrap().totp_secret.as_deref(), Some(secret.as_str()));

    // password alone gives no session, only the 2FA step
    let (st, h, body) = send(&s, login_form("admin", "correct-horse-1")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(!h.iter().any(|(k, _)| k == "set-cookie"));
    let token = hidden_token(&body);

    // wrong code: rejected, and the token is used up
    let req = |t: &str, c: &str| {
        Request::builder()
            .method("POST")
            .uri("/login/2fa")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("token={t}&code={c}")))
            .unwrap()
    };
    let (_, h, _) = send(&s, req(&token, "123456")).await;
    assert!(!h.iter().any(|(k, _)| k == "set-cookie"));
    let (_, h, _) = send(&s, req(&token, &code)).await;
    assert!(!h.iter().any(|(k, _)| k == "set-cookie"), "token must be single use");

    // fresh password + right code: logged in
    let (_, _, body) = send(&s, login_form("admin", "correct-horse-1")).await;
    let code = auth::totp(&secret, "admin").unwrap().generate_current().to_string();
    let (st, h, _) = send(&s, req(&hidden_token(&body), &code)).await;
    assert_eq!(st, StatusCode::SEE_OTHER);
    assert!(h.iter().any(|(k, _)| k == "set-cookie"));
}

#[tokio::test]
async fn password_change_and_admins() {
    let s = state();
    let (cookie, csrf) = login(&s).await;
    // second session that must be logged out by the password change
    let (_, h, _) = send(&s, login_form("admin", "correct-horse-1")).await;
    let other = h.iter().find(|(k, _)| k == "set-cookie").unwrap().1.split(';').next().unwrap().to_string();
    assert_eq!(send(&s, get("/", &other)).await.0, StatusCode::OK);

    send(&s, form("/account/password", &cookie, &format!("csrf={csrf}&current=wrong&password=new-password-9&password2=new-password-9"))).await;
    let a = s.db.admin_by_name("admin").unwrap().unwrap();
    assert!(auth::verify_password("correct-horse-1", &a.password_hash));

    send(&s, form("/account/password", &cookie, &format!("csrf={csrf}&current=correct-horse-1&password=new-password-9&password2=new-password-9"))).await;
    let a = s.db.admin_by_name("admin").unwrap().unwrap();
    assert!(auth::verify_password("new-password-9", &a.password_hash));
    assert_eq!(send(&s, get("/", &other)).await.0, StatusCode::SEE_OTHER, "other session logged out");
    assert_eq!(send(&s, get("/", &cookie)).await.0, StatusCode::OK, "own session kept");

    send(&s, form("/account/admins", &cookie, &format!("csrf={csrf}&username=zweit&password=0123456789ab"))).await;
    assert_eq!(s.db.admins().unwrap().len(), 2);
    let (st, _, body) = send(&s, get("/account", &cookie)).await;
    assert_eq!(st, StatusCode::OK);
    assert!(body.contains("zweit") && body.contains("data:image/png;base64,"));
    let me = s.db.admin_by_name("admin").unwrap().unwrap().id;
    send(&s, form(&format!("/account/admins/{me}/delete"), &cookie, &format!("csrf={csrf}"))).await;
    assert_eq!(s.db.admins().unwrap().len(), 2, "cannot delete yourself");
}

#[tokio::test]
async fn remote_actions_roundtrip() {
    let s = state();
    let (cookie, csrf) = login(&s).await;
    let dev = Identity::generate().unwrap();
    let req = EnrollRequest {
        public_key: dev.public_key(),
        hostname: "pc".into(),
        os: String::new(),
        agent_version: String::new(),
        hardware: String::new(),
    };
    let (_, _, body) = send(&s, signed(&dev, "POST", "/api/v1/enroll", serde_json::to_vec(&req).unwrap())).await;
    let code = serde_json::from_str::<EnrollResponse>(&body).unwrap().code;
    s.db.create_group("7b").unwrap();
    let gid = s.db.groups().unwrap()[0].id;
    send(&s, form("/adopt", &cookie, &format!("csrf={csrf}&code={code}&role=student&group={gid}"))).await;
    let did = s.db.device_by_key(&dev.public_key()).unwrap().unwrap().id;

    // unknown kinds are refused, the same action is only queued once
    send(&s, form(&format!("/devices/{did}/action"), &cookie, &format!("csrf={csrf}&kind=rm-rf"))).await;
    send(&s, form(&format!("/devices/{did}/action"), &cookie, &format!("csrf={csrf}&kind=reboot"))).await;
    send(&s, form(&format!("/groups/{gid}/action"), &cookie, &format!("csrf={csrf}&kind=reboot"))).await;
    let ManifestResponse::Adopted { manifest: m } = manifest(&s, &dev).await else { panic!() };
    let m = m.open(&s.key.public_key()).unwrap();
    assert_eq!(m.actions.len(), 1);
    assert_eq!(m.actions[0].kind, "reboot");

    let report = Report { revision: Some(m.revision), ok: true, done_actions: vec![m.actions[0].id], ..Default::default() };
    send(&s, signed(&dev, "POST", "/api/v1/report", serde_json::to_vec(&report).unwrap())).await;
    let ManifestResponse::Adopted { manifest: m2 } = manifest(&s, &dev).await else { panic!() };
    assert!(m2.open(&s.key.public_key()).unwrap().actions.is_empty());
    let (st, _, body) = send(&s, get(&format!("/devices/{did}"), &cookie)).await;
    assert_eq!(st, StatusCode::OK);
    assert!(body.contains("ausgeführt"));
}
