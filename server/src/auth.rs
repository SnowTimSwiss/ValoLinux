//! Admin login: argon2 password hashes, server-side sessions, CSRF tokens.

use crate::{Shared, util};
use axum::extract::FromRequestParts;
use axum::http::{HeaderMap, header, request::Parts};
use axum::response::{IntoResponse, Redirect, Response};

pub const COOKIE: &str = "valo_session";
const SESSION_SECS: i64 = 12 * 3600;

pub fn hash_password(pw: &str) -> String {
    use argon2::password_hash::PasswordHasher;
    argon2::Argon2::default().hash_password(pw.as_bytes()).expect("argon2").to_string()
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    use argon2::password_hash::{PasswordVerifier, phc::PasswordHash};
    PasswordHash::new(hash)
        .map(|h| argon2::Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

/// Create a session and return the Set-Cookie header value.
pub fn login_cookie(state: &Shared, admin_id: i64) -> Result<String, rusqlite::Error> {
    let token = util::random_token();
    let csrf = util::random_token();
    state.db.create_session(&token, admin_id, &csrf, valo_proto::now_unix() + SESSION_SECS)?;
    let secure = if state.secure_cookies { "; Secure" } else { "" };
    Ok(format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={SESSION_SECS}{secure}"))
}

pub fn logout_cookie() -> String {
    format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0")
}

pub fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_string())
}

/// A logged-in admin. Handlers that take this are only reachable after login.
pub struct Admin {
    pub username: String,
    pub csrf: String,
}

impl Admin {
    /// Forms must echo the session's CSRF token.
    pub fn check_csrf(&self, form: &util::FormData) -> Result<(), Response> {
        if form.get("csrf") == Some(self.csrf.as_str()) {
            Ok(())
        } else {
            Err((axum::http::StatusCode::FORBIDDEN, "Ungültiges Formular (CSRF). Bitte Seite neu laden.").into_response())
        }
    }
}

impl FromRequestParts<Shared> for Admin {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Self::Rejection> {
        if state.db.admin_count().unwrap_or(0) == 0 {
            return Err(Redirect::to("/setup").into_response());
        }
        let session = session_token(&parts.headers).and_then(|t| state.db.session(&t).ok().flatten());
        match session {
            Some((username, csrf)) => Ok(Admin { username, csrf }),
            None => Err(Redirect::to("/login").into_response()),
        }
    }
}
