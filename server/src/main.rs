//! ValoLinux management server: web UI for admins plus the API the device agents poll.

// Handlers return axum `Response` as error type; that is idiomatic here.
#![allow(clippy::result_large_err)]

mod api;
mod auth;
mod db;
mod fields;
mod util;
mod web;

use axum::Router;
use std::path::PathBuf;
use std::sync::Arc;
use valo_proto::Identity;

pub struct AppState {
    pub db: db::Db,
    pub key: Identity,
    /// Set the `Secure` flag on cookies (when served via HTTPS).
    pub secure_cookies: bool,
    /// Failed logins per username: (count, window start). In memory on purpose.
    pub failed_logins: std::sync::Mutex<std::collections::HashMap<String, (u32, i64)>>,
    /// Uploaded files (wallpapers), named by sha256.
    pub assets_dir: PathBuf,
}

pub type Shared = Arc<AppState>;

pub fn app(state: Shared) -> Router {
    Router::new().merge(web::routes()).merge(api::routes()).with_state(state)
}

fn load_or_create_key(path: &std::path::Path) -> std::io::Result<Identity> {
    if path.exists() {
        let seed = std::fs::read(path)?;
        return Identity::from_seed(&seed).map_err(|e| std::io::Error::other(e.to_string()));
    }
    let id = Identity::generate().map_err(|e| std::io::Error::other(e.to_string()))?;
    write_private(path, &id.seed())?;
    tracing::info!("created new server signing key at {}", path.display());
    Ok(id)
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    f.write_all(data)
}

#[cfg(unix)]
fn restrict_data_dir(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let set = |p: &std::path::Path, mode| {
        if let Err(e) = std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)) {
            tracing::warn!("cannot restrict permissions of {}: {e}", p.display());
        }
    };
    set(dir, 0o700);
    for f in ["valo.db", "valo.db-wal", "valo.db-shm"] {
        let p = dir.join(f);
        if p.exists() {
            set(&p, 0o600);
        }
    }
}

/// `valo-server --healthcheck`: used by the container HEALTHCHECK (no curl in the image).
fn healthcheck() -> ! {
    use std::io::{Read, Write};
    let listen = std::env::var("VALO_LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let port = listen.rsplit(':').next().unwrap_or("8080");
    let ok = std::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .and_then(|mut s| {
            s.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
            s.write_all(b"GET /healthz HTTP/1.0\r\n\r\n")?;
            let mut buf = String::new();
            s.read_to_string(&mut buf)?;
            Ok(buf.starts_with("HTTP/1.0 200") || buf.starts_with("HTTP/1.1 200"))
        })
        .unwrap_or(false);
    std::process::exit(if ok { 0 } else { 1 })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|a| a == "--healthcheck") {
        healthcheck();
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let data_dir = PathBuf::from(std::env::var("VALO_DATA_DIR").unwrap_or_else(|_| "./data".into()));
    let listen = std::env::var("VALO_LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let secure_cookies = std::env::var("VALO_SECURE_COOKIES").is_ok_and(|v| v == "1" || v == "true");
    std::fs::create_dir_all(&data_dir)?;

    let key = load_or_create_key(&data_dir.join("server.key"))?;
    let db = db::Db::open(&data_dir.join("valo.db"))?;
    // the database holds per-device admin passwords: owner only
    restrict_data_dir(&data_dir);
    tracing::info!("server public key: {}", key.public_key());

    let assets_dir = data_dir.join("assets");
    std::fs::create_dir_all(&assets_dir)?;
    let state = Arc::new(AppState { db, key, secure_cookies, failed_logins: Default::default(), assets_dir });
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    tracing::info!("listening on http://{listen}");
    axum::serve(listener, app(state)).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
