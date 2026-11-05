//! SQLite storage. One connection behind a mutex: a school has hundreds of
//! devices, not millions, so this stays simple on purpose.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use valo_proto::{CatalogApp, Settings, now_unix};

pub struct Db(Mutex<Connection>);

pub type Result<T> = rusqlite::Result<T>;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS admins (
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    token TEXT PRIMARY KEY,
    admin_id INTEGER NOT NULL REFERENCES admins(id) ON DELETE CASCADE,
    csrf TEXT NOT NULL,
    expires INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS pending_logins (
    token TEXT PRIMARY KEY,
    admin_id INTEGER NOT NULL REFERENCES admins(id) ON DELETE CASCADE,
    expires INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS groups (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS devices (
    id INTEGER PRIMARY KEY,
    public_key TEXT NOT NULL UNIQUE,
    code TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    name TEXT NOT NULL DEFAULT '',
    role TEXT NOT NULL DEFAULT 'student',
    group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL,
    hostname TEXT NOT NULL DEFAULT '',
    os TEXT NOT NULL DEFAULT '',
    hardware TEXT NOT NULL DEFAULT '',
    agent_version TEXT NOT NULL DEFAULT '',
    admin_password TEXT,
    admin_password_hash TEXT,
    created INTEGER NOT NULL,
    last_seen INTEGER,
    last_revision TEXT,
    last_ok INTEGER,
    last_messages TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS policies (
    scope TEXT PRIMARY KEY,
    json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS catalog (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    flatpak_ref TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT ''
);
"#;

/// Initial catalog. Only these (and what the admin adds) can be installed.
/// Flatpak ids are the Flathub ids as far as known; verify before a pilot.
const DEFAULT_CATALOG: &[(&str, &str, &str, &str)] = &[
    ("libreoffice", "LibreOffice", "org.libreoffice.LibreOffice", "Office-Suite (Fallback zu ArtCraft)"),
    ("gimp", "GIMP", "org.gimp.GIMP", "Bildbearbeitung"),
    ("inkscape", "Inkscape", "org.inkscape.Inkscape", "Vektorgrafik"),
    ("krita", "Krita", "org.kde.krita", "Malen und Zeichnen"),
    ("audacity", "Audacity", "org.audacityteam.Audacity", "Audio aufnehmen und schneiden"),
    ("vlc", "VLC", "org.videolan.VLC", "Medienwiedergabe"),
    ("calculator", "Rechner", "org.gnome.Calculator", "Taschenrechner"),
];

#[derive(Debug, Clone)]
pub struct Device {
    pub id: i64,
    pub public_key: String,
    pub code: String,
    pub status: String,
    pub name: String,
    pub role: String,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub hostname: String,
    pub os: String,
    pub hardware: String,
    pub agent_version: String,
    pub admin_password: Option<String>,
    pub admin_password_hash: Option<String>,
    pub created: i64,
    pub last_seen: Option<i64>,
    pub last_revision: Option<String>,
    pub last_ok: Option<bool>,
    pub last_messages: String,
}

impl Device {
    pub fn adopted(&self) -> bool {
        self.status == "adopted"
    }
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() { &self.hostname } else { &self.name }
    }
}

#[derive(Debug, Clone)]
pub struct AdminRow {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub totp_secret: Option<String>,
}

fn row_admin(r: &rusqlite::Row) -> Result<AdminRow> {
    Ok(AdminRow { id: r.get(0)?, username: r.get(1)?, password_hash: r.get(2)?, totp_secret: r.get(3)? })
}

#[derive(Debug, Clone)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub device_count: i64,
}

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub app: CatalogApp,
    pub description: String,
}

const DEVICE_COLS: &str = "d.id, d.public_key, d.code, d.status, d.name, d.role, d.group_id, g.name, d.hostname, d.os, \
    d.hardware, d.agent_version, d.admin_password, d.admin_password_hash, d.created, d.last_seen, d.last_revision, \
    d.last_ok, d.last_messages";

fn row_device(r: &rusqlite::Row) -> Result<Device> {
    Ok(Device {
        id: r.get(0)?,
        public_key: r.get(1)?,
        code: r.get(2)?,
        status: r.get(3)?,
        name: r.get(4)?,
        role: r.get(5)?,
        group_id: r.get(6)?,
        group_name: r.get(7)?,
        hostname: r.get(8)?,
        os: r.get(9)?,
        hardware: r.get(10)?,
        agent_version: r.get(11)?,
        admin_password: r.get(12)?,
        admin_password_hash: r.get(13)?,
        created: r.get(14)?,
        last_seen: r.get(15)?,
        last_revision: r.get(16)?,
        last_ok: r.get::<_, Option<i64>>(17)?.map(|v| v != 0),
        last_messages: r.get(18)?,
    })
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        conn.execute_batch(SCHEMA)?;
        // migrations for databases created by older versions
        let has_totp: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('admins') WHERE name = 'totp_secret'")?
            .exists([])?;
        if !has_totp {
            conn.execute_batch("ALTER TABLE admins ADD COLUMN totp_secret TEXT")?;
        }
        for (id, name, fref, desc) in DEFAULT_CATALOG {
            conn.execute(
                "INSERT OR IGNORE INTO catalog (id, name, flatpak_ref, description) VALUES (?1, ?2, ?3, ?4)",
                params![id, name, fref, desc],
            )?;
        }
        Ok(Db(Mutex::new(conn)))
    }

    fn c(&self) -> MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ----- admins & sessions -------------------------------------------------

    pub fn admin_count(&self) -> Result<i64> {
        self.c().query_row("SELECT COUNT(*) FROM admins", [], |r| r.get(0))
    }

    pub fn create_admin(&self, username: &str, password_hash: &str) -> Result<i64> {
        let c = self.c();
        c.execute(
            "INSERT INTO admins (username, password_hash) VALUES (?1, ?2)",
            params![username, password_hash],
        )?;
        Ok(c.last_insert_rowid())
    }

    pub fn admin_by_name(&self, username: &str) -> Result<Option<AdminRow>> {
        self.c()
            .query_row(
                "SELECT id, username, password_hash, totp_secret FROM admins WHERE username = ?1",
                [username],
                row_admin,
            )
            .optional()
    }

    pub fn admin_by_id(&self, id: i64) -> Result<Option<AdminRow>> {
        self.c()
            .query_row("SELECT id, username, password_hash, totp_secret FROM admins WHERE id = ?1", [id], row_admin)
            .optional()
    }

    pub fn admins(&self) -> Result<Vec<AdminRow>> {
        let c = self.c();
        let mut st = c.prepare("SELECT id, username, password_hash, totp_secret FROM admins ORDER BY lower(username)")?;
        st.query_map([], row_admin)?.collect()
    }

    pub fn set_admin_login_password(&self, id: i64, hash: &str) -> Result<()> {
        let c = self.c();
        c.execute("UPDATE admins SET password_hash = ?2 WHERE id = ?1", params![id, hash])?;
        Ok(())
    }

    pub fn set_admin_totp(&self, id: i64, secret: Option<&str>) -> Result<()> {
        self.c().execute("UPDATE admins SET totp_secret = ?2 WHERE id = ?1", params![id, secret])?;
        Ok(())
    }

    pub fn delete_admin(&self, id: i64) -> Result<()> {
        self.c().execute("DELETE FROM admins WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn create_pending_login(&self, token: &str, admin_id: i64, expires: i64) -> Result<()> {
        let c = self.c();
        c.execute("DELETE FROM pending_logins WHERE expires < ?1", [now_unix()])?;
        c.execute(
            "INSERT INTO pending_logins (token, admin_id, expires) VALUES (?1, ?2, ?3)",
            params![token, admin_id, expires],
        )?;
        Ok(())
    }

    /// Single use: the token is deleted whether or not the 2FA code turns out right.
    pub fn take_pending_login(&self, token: &str) -> Result<Option<i64>> {
        let c = self.c();
        let id: Option<i64> = c
            .query_row(
                "SELECT admin_id FROM pending_logins WHERE token = ?1 AND expires > ?2",
                params![token, now_unix()],
                |r| r.get(0),
            )
            .optional()?;
        c.execute("DELETE FROM pending_logins WHERE token = ?1", [token])?;
        Ok(id)
    }

    pub fn create_session(&self, token: &str, admin_id: i64, csrf: &str, expires: i64) -> Result<()> {
        let c = self.c();
        c.execute("DELETE FROM sessions WHERE expires < ?1", [now_unix()])?;
        c.execute(
            "INSERT INTO sessions (token, admin_id, csrf, expires) VALUES (?1, ?2, ?3, ?4)",
            params![token, admin_id, csrf, expires],
        )?;
        Ok(())
    }

    /// Returns (admin id, csrf token) for a valid session.
    pub fn session(&self, token: &str) -> Result<Option<(i64, String)>> {
        self.c()
            .query_row(
                "SELECT s.admin_id, s.csrf FROM sessions s WHERE s.token = ?1 AND s.expires > ?2",
                params![token, now_unix()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
    }

    /// Log out everywhere (e.g. after a password change), except `keep`.
    pub fn delete_sessions_of(&self, admin_id: i64, keep: &str) -> Result<()> {
        self.c().execute("DELETE FROM sessions WHERE admin_id = ?1 AND token != ?2", params![admin_id, keep])?;
        Ok(())
    }

    pub fn delete_session(&self, token: &str) -> Result<()> {
        self.c().execute("DELETE FROM sessions WHERE token = ?1", [token])?;
        Ok(())
    }

    // ----- devices -----------------------------------------------------------

    /// Register a new device or refresh an existing one. Returns the device.
    pub fn upsert_enrolling_device(
        &self,
        public_key: &str,
        new_code: impl Fn() -> String,
        hostname: &str,
        os: &str,
        hardware: &str,
        agent_version: &str,
    ) -> Result<Device> {
        {
            let c = self.c();
            let exists: Option<i64> = c
                .query_row("SELECT id FROM devices WHERE public_key = ?1", [public_key], |r| r.get(0))
                .optional()?;
            match exists {
                Some(id) => {
                    c.execute(
                        "UPDATE devices SET hostname = ?2, os = ?3, hardware = ?4, agent_version = ?5, last_seen = ?6 \
                         WHERE id = ?1",
                        params![id, hostname, os, hardware, agent_version, now_unix()],
                    )?;
                }
                None => {
                    // codes must be unique among pending devices
                    let mut code = new_code();
                    while c
                        .query_row(
                            "SELECT 1 FROM devices WHERE code = ?1 AND status = 'pending'",
                            [&code],
                            |_| Ok(()),
                        )
                        .optional()?
                        .is_some()
                    {
                        code = new_code();
                    }
                    c.execute(
                        "INSERT INTO devices (public_key, code, hostname, os, hardware, agent_version, created, last_seen) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                        params![public_key, code, hostname, os, hardware, agent_version, now_unix()],
                    )?;
                }
            }
        }
        Ok(self.device_by_key(public_key)?.expect("device just written"))
    }

    pub fn device_by_key(&self, public_key: &str) -> Result<Option<Device>> {
        self.c()
            .query_row(
                &format!("SELECT {DEVICE_COLS} FROM devices d LEFT JOIN groups g ON g.id = d.group_id WHERE d.public_key = ?1"),
                [public_key],
                row_device,
            )
            .optional()
    }

    pub fn device(&self, id: i64) -> Result<Option<Device>> {
        self.c()
            .query_row(
                &format!("SELECT {DEVICE_COLS} FROM devices d LEFT JOIN groups g ON g.id = d.group_id WHERE d.id = ?1"),
                [id],
                row_device,
            )
            .optional()
    }

    pub fn pending_by_code(&self, code: &str) -> Result<Option<Device>> {
        self.c()
            .query_row(
                &format!(
                    "SELECT {DEVICE_COLS} FROM devices d LEFT JOIN groups g ON g.id = d.group_id \
                     WHERE d.code = ?1 AND d.status = 'pending'"
                ),
                [code],
                row_device,
            )
            .optional()
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        let c = self.c();
        let mut st = c.prepare(&format!(
            "SELECT {DEVICE_COLS} FROM devices d LEFT JOIN groups g ON g.id = d.group_id \
             ORDER BY d.status DESC, lower(CASE WHEN d.name = '' THEN d.hostname ELSE d.name END)"
        ))?;
        st.query_map([], row_device)?.collect()
    }

    pub fn adopt(
        &self,
        id: i64,
        name: &str,
        role: &str,
        group_id: Option<i64>,
        admin_password: &str,
        admin_password_hash: &str,
    ) -> Result<()> {
        self.c().execute(
            "UPDATE devices SET status = 'adopted', name = ?2, role = ?3, group_id = ?4, admin_password = ?5, \
             admin_password_hash = ?6 WHERE id = ?1",
            params![id, name, role, group_id, admin_password, admin_password_hash],
        )?;
        Ok(())
    }

    pub fn update_device(&self, id: i64, name: &str, role: &str, group_id: Option<i64>) -> Result<()> {
        self.c().execute(
            "UPDATE devices SET name = ?2, role = ?3, group_id = ?4 WHERE id = ?1",
            params![id, name, role, group_id],
        )?;
        Ok(())
    }

    pub fn set_admin_password(&self, id: i64, password: &str, hash: &str) -> Result<()> {
        self.c().execute(
            "UPDATE devices SET admin_password = ?2, admin_password_hash = ?3 WHERE id = ?1",
            params![id, password, hash],
        )?;
        Ok(())
    }

    pub fn delete_device(&self, id: i64) -> Result<()> {
        let c = self.c();
        c.execute("DELETE FROM policies WHERE scope = ?1", [format!("device:{id}")])?;
        c.execute("DELETE FROM devices WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn touch_device(&self, id: i64) -> Result<()> {
        self.c().execute("UPDATE devices SET last_seen = ?2 WHERE id = ?1", params![id, now_unix()])?;
        Ok(())
    }

    pub fn record_report(&self, id: i64, r: &valo_proto::Report) -> Result<()> {
        self.c().execute(
            "UPDATE devices SET last_seen = ?2, last_revision = ?3, last_ok = ?4, last_messages = ?5, \
             agent_version = ?6, hostname = CASE WHEN ?7 = '' THEN hostname ELSE ?7 END, \
             os = CASE WHEN ?8 = '' THEN os ELSE ?8 END WHERE id = ?1",
            params![
                id,
                now_unix(),
                r.revision,
                r.ok as i64,
                r.messages.join("\n"),
                r.agent_version,
                r.hostname,
                r.os
            ],
        )?;
        Ok(())
    }

    // ----- groups ------------------------------------------------------------

    pub fn groups(&self) -> Result<Vec<Group>> {
        let c = self.c();
        let mut st = c.prepare(
            "SELECT g.id, g.name, (SELECT COUNT(*) FROM devices d WHERE d.group_id = g.id AND d.status = 'adopted') \
             FROM groups g ORDER BY lower(g.name)",
        )?;
        st.query_map([], |r| Ok(Group { id: r.get(0)?, name: r.get(1)?, device_count: r.get(2)? }))?
            .collect()
    }

    pub fn group(&self, id: i64) -> Result<Option<Group>> {
        Ok(self.groups()?.into_iter().find(|g| g.id == id))
    }

    pub fn create_group(&self, name: &str) -> Result<i64> {
        let c = self.c();
        c.execute("INSERT INTO groups (name) VALUES (?1)", [name])?;
        Ok(c.last_insert_rowid())
    }

    pub fn rename_group(&self, id: i64, name: &str) -> Result<()> {
        self.c().execute("UPDATE groups SET name = ?2 WHERE id = ?1", params![id, name])?;
        Ok(())
    }

    pub fn delete_group(&self, id: i64) -> Result<()> {
        let c = self.c();
        c.execute("DELETE FROM policies WHERE scope = ?1", [format!("group:{id}")])?;
        c.execute("DELETE FROM groups WHERE id = ?1", [id])?;
        Ok(())
    }

    // ----- policies ----------------------------------------------------------

    pub fn policy(&self, scope: &str) -> Result<Settings> {
        let json: Option<String> = self
            .c()
            .query_row("SELECT json FROM policies WHERE scope = ?1", [scope], |r| r.get(0))
            .optional()?;
        Ok(json.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
    }

    pub fn set_policy(&self, scope: &str, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string(settings).expect("settings serialize");
        self.c().execute(
            "INSERT INTO policies (scope, json) VALUES (?1, ?2) ON CONFLICT(scope) DO UPDATE SET json = ?2",
            params![scope, json],
        )?;
        Ok(())
    }

    /// The settings layers for a device, lowest priority first.
    pub fn layers_for(&self, d: &Device) -> Result<Vec<(String, Settings)>> {
        let mut scopes = vec!["global".to_string(), format!("role:{}", d.role)];
        if let Some(g) = d.group_id {
            scopes.push(format!("group:{g}"));
        }
        scopes.push(format!("device:{}", d.id));
        scopes.into_iter().map(|s| Ok((s.clone(), self.policy(&s)?))).collect()
    }

    // ----- catalog -----------------------------------------------------------

    pub fn catalog(&self) -> Result<Vec<CatalogEntry>> {
        let c = self.c();
        let mut st = c.prepare("SELECT id, name, flatpak_ref, description FROM catalog ORDER BY lower(name)")?;
        st.query_map([], |r| {
            Ok(CatalogEntry {
                app: CatalogApp { id: r.get(0)?, name: r.get(1)?, flatpak_ref: r.get(2)? },
                description: r.get(3)?,
            })
        })?
        .collect()
    }

    pub fn add_catalog(&self, id: &str, name: &str, flatpak_ref: &str, description: &str) -> Result<()> {
        self.c().execute(
            "INSERT INTO catalog (id, name, flatpak_ref, description) VALUES (?1, ?2, ?3, ?4)",
            params![id, name, flatpak_ref, description],
        )?;
        Ok(())
    }

    pub fn delete_catalog(&self, id: &str) -> Result<()> {
        self.c().execute("DELETE FROM catalog WHERE id = ?1", [id])?;
        Ok(())
    }
}
