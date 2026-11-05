//! Agent API. Every request is signed with the device key (see valo_proto::request_message).
//! Plain HTTP(S) for now; the transport can move to iroh later without changing these messages.

use crate::db::Device;
use crate::{Shared, util};
use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use valo_proto::{
    AGENT_HEADER_KEY, AGENT_HEADER_SIG, AGENT_HEADER_TIME, EnrollRequest, EnrollResponse, MAX_CLOCK_SKEW_SECS,
    Manifest, ManifestResponse, Report, Settings, SignedManifest, now_unix, request_message,
};

/// Upper bound for devices waiting for adoption (stops a misbehaving client filling the list).
const MAX_PENDING: usize = 1000;

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/v1/server-key", get(server_key))
        .route("/api/v1/enroll", post(enroll))
        .route("/api/v1/manifest", get(manifest))
        .route("/api/v1/report", post(report))
}

fn err(code: StatusCode, msg: &str) -> Response {
    (code, msg.to_string()).into_response()
}

fn internal<E: std::fmt::Display>(e: E) -> Response {
    tracing::error!("internal error: {e}");
    err(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}

/// Check the request signature and return the device public key.
fn verify_agent(headers: &HeaderMap, method: &str, path: &str, body: &[u8]) -> Result<String, Response> {
    let h = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
    let (Some(key), Some(time), Some(sig)) = (h(AGENT_HEADER_KEY), h(AGENT_HEADER_TIME), h(AGENT_HEADER_SIG)) else {
        return Err(err(StatusCode::UNAUTHORIZED, "missing signature headers"));
    };
    let time: i64 = time.parse().map_err(|_| err(StatusCode::UNAUTHORIZED, "bad time"))?;
    if (now_unix() - time).abs() > MAX_CLOCK_SKEW_SECS {
        return Err(err(StatusCode::UNAUTHORIZED, "clock skew too large, check device time"));
    }
    valo_proto::verify(&key, &request_message(method, path, time, body), &sig)
        .map_err(|_| err(StatusCode::UNAUTHORIZED, "bad signature"))?;
    Ok(key)
}

async fn server_key(State(s): State<Shared>) -> String {
    s.key.public_key()
}

async fn enroll(State(s): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let key = match verify_agent(&headers, "POST", "/api/v1/enroll", &body) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let Ok(req) = serde_json::from_slice::<EnrollRequest>(&body) else {
        return err(StatusCode::BAD_REQUEST, "bad json");
    };
    if req.public_key != key {
        return err(StatusCode::BAD_REQUEST, "public_key must match the signing key");
    }
    let known = s.db.device_by_key(&key).ok().flatten().is_some();
    if !known {
        let pending = s.db.devices().map(|d| d.iter().filter(|d| !d.adopted()).count()).unwrap_or(0);
        if pending >= MAX_PENDING {
            return err(StatusCode::TOO_MANY_REQUESTS, "too many devices waiting for adoption");
        }
    }
    let clip = |v: &str| v.chars().take(200).collect::<String>();
    match s.db.upsert_enrolling_device(
        &key,
        util::adoption_code,
        &clip(&req.hostname),
        &clip(&req.os),
        &clip(&req.hardware),
        &clip(&req.agent_version),
    ) {
        Ok(d) => Json(EnrollResponse { code: d.code, server_key: s.key.public_key() }).into_response(),
        Err(e) => internal(e),
    }
}

/// Build the resolved, signed manifest for an adopted device.
pub fn build_manifest(s: &Shared, d: &Device) -> Result<SignedManifest, rusqlite::Error> {
    let layers = s.db.layers_for(d)?;
    let settings = Settings::resolve(layers.iter().map(|(_, l)| l));
    let wanted = settings.apps.clone().unwrap_or_default();
    let apps = s
        .db
        .catalog()?
        .into_iter()
        .map(|e| e.app)
        .filter(|a| wanted.contains(&a.id))
        .collect();
    let mut m = Manifest {
        revision: String::new(),
        device_name: d.display_name().to_string(),
        role: d.role.clone(),
        group: d.group_name.clone(),
        settings,
        admin_password_hash: d.admin_password_hash.clone(),
        apps,
        issued_at: now_unix(),
    };
    m.compute_revision();
    Ok(SignedManifest::sign(&m, &s.key))
}

async fn manifest(State(s): State<Shared>, headers: HeaderMap) -> Response {
    let key = match verify_agent(&headers, "GET", "/api/v1/manifest", b"") {
        Ok(k) => k,
        Err(r) => return r,
    };
    let device = match s.db.device_by_key(&key) {
        Ok(Some(d)) => d,
        Ok(None) => return err(StatusCode::NOT_FOUND, "unknown device, enroll first"),
        Err(e) => return internal(e),
    };
    let _ = s.db.touch_device(device.id);
    if !device.adopted() {
        return Json(ManifestResponse::Pending { code: device.code }).into_response();
    }
    match build_manifest(&s, &device) {
        Ok(manifest) => Json(ManifestResponse::Adopted { manifest }).into_response(),
        Err(e) => internal(e),
    }
}

async fn report(State(s): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let key = match verify_agent(&headers, "POST", "/api/v1/report", &body) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let Ok(mut r) = serde_json::from_slice::<Report>(&body) else {
        return err(StatusCode::BAD_REQUEST, "bad json");
    };
    r.messages.truncate(50);
    match s.db.device_by_key(&key) {
        Ok(Some(d)) => match s.db.record_report(d.id, &r) {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(e) => internal(e),
        },
        Ok(None) => err(StatusCode::NOT_FOUND, "unknown device"),
        Err(e) => internal(e),
    }
}
