//! Shared types for the ValoLinux server and agent.
//!
//! Nothing in here knows about Debian: the agent maps [`Settings`] onto the
//! concrete system through its own platform backend.

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const AGENT_HEADER_KEY: &str = "x-valo-device";
pub const AGENT_HEADER_TIME: &str = "x-valo-time";
pub const AGENT_HEADER_SIG: &str = "x-valo-signature";
/// Maximum clock difference accepted for signed agent requests.
pub const MAX_CLOCK_SKEW_SECS: i64 = 300;

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// What happens to the user's home directory at logout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HomeMode {
    /// Delete everything at logout (stateless device, default).
    Wipe,
    /// Keep files on the device.
    Keep,
}

/// Device settings. Every field is optional so the same type can describe a
/// partial override (role, group, device) as well as the resolved result.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// XKB layout, optionally with variant: "ch", "ch:fr", "de", "us".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyboard: Option<String>,
    /// e.g. "de_CH.UTF-8"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// IANA name, e.g. "Europe/Zurich"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_mode: Option<HomeMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_homepage: Option<String>,
    /// "ecosia" | "duckduckgo" | "startpage" | "google"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_adblock: Option<bool>,
    /// May users install Firefox add-ons themselves?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_allow_addons: Option<bool>,
    /// Catalog app ids to install.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apps: Option<Vec<String>>,
    /// May the user install apps from the catalog themselves?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_app_install: Option<bool>,
    /// May the user change keyboard, wallpaper and similar personal settings?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_user_settings: Option<bool>,
    /// May the user lock the screen (Super+L)?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_lock_screen: Option<bool>,
    /// Update ring for image releases: "test" | "pilot" | "all"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_ring: Option<String>,
}

impl Settings {
    /// Built-in defaults; every field is set.
    pub fn defaults() -> Self {
        Settings {
            keyboard: Some("ch".into()),
            locale: Some("de_CH.UTF-8".into()),
            timezone: Some("Europe/Zurich".into()),
            home_mode: Some(HomeMode::Wipe),
            browser_homepage: Some("https://www.ecosia.org".into()),
            browser_search: Some("ecosia".into()),
            browser_adblock: Some(true),
            browser_allow_addons: Some(false),
            apps: Some(vec![]),
            allow_app_install: Some(false),
            allow_user_settings: Some(false),
            allow_lock_screen: Some(true),
            update_ring: Some("all".into()),
        }
    }

    /// Fields set in `other` replace the ones in `self`.
    pub fn overlay(&mut self, other: &Settings) {
        macro_rules! take {
            ($($f:ident),*) => { $( if other.$f.is_some() { self.$f = other.$f.clone(); } )* };
        }
        take!(
            keyboard,
            locale,
            timezone,
            home_mode,
            browser_homepage,
            browser_search,
            browser_adblock,
            browser_allow_addons,
            apps,
            allow_app_install,
            allow_user_settings,
            allow_lock_screen,
            update_ring
        );
    }

    /// Resolve defaults + layers in order (global, role, group, device).
    pub fn resolve<'a>(layers: impl IntoIterator<Item = &'a Settings>) -> Settings {
        let mut s = Settings::defaults();
        for l in layers {
            s.overlay(l);
        }
        s
    }

    /// Split "ch:fr" into ("ch", "fr").
    pub fn keyboard_parts(&self) -> (String, String) {
        let kb = self.keyboard.clone().unwrap_or_else(|| "us".into());
        match kb.split_once(':') {
            Some((l, v)) => (l.to_string(), v.to_string()),
            None => (kb, String::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollRequest {
    /// Base64 ed25519 public key; this is the device identity.
    pub public_key: String,
    pub hostname: String,
    pub os: String,
    pub agent_version: String,
    #[serde(default)]
    pub hardware: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollResponse {
    /// Short code shown on the device, typed by the admin in the web UI.
    pub code: String,
    /// Base64 public key the server signs manifests with. Pinned by the agent.
    pub server_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogApp {
    pub id: String,
    pub name: String,
    /// Flatpak application id, e.g. "org.libreoffice.LibreOffice".
    pub flatpak_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Changes whenever anything in the manifest changes.
    pub revision: String,
    pub device_name: String,
    /// "student" | "teacher"
    pub role: String,
    pub group: Option<String>,
    pub settings: Settings,
    /// crypt(3) SHA-512 hash for the local admin account.
    pub admin_password_hash: Option<String>,
    /// Apps referenced by `settings.apps`, resolved from the catalog.
    pub apps: Vec<CatalogApp>,
    pub issued_at: i64,
}

impl Manifest {
    pub fn compute_revision(&mut self) {
        self.revision = String::new();
        let bytes = serde_json::to_vec(&ManifestForHash(self)).expect("manifest serializes");
        self.revision = hex(&Sha256::digest(bytes))[..16].to_string();
    }
}

/// Hash input excludes `issued_at` so the revision is stable.
#[derive(Serialize)]
struct ManifestForHash<'a>(#[serde(serialize_with = "ser_without_time")] &'a Manifest);

fn ser_without_time<S: serde::Serializer>(m: &&Manifest, s: S) -> Result<S::Ok, S::Error> {
    (&m.device_name, &m.role, &m.group, &m.settings, &m.admin_password_hash, &m.apps).serialize(s)
}

/// Manifest JSON plus server signature over exactly these bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedManifest {
    pub payload: String,
    pub signature: String,
}

/// Answer to a manifest request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ManifestResponse {
    /// Not adopted yet; show this code on the device.
    Pending { code: String },
    Adopted { manifest: SignedManifest },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    pub revision: Option<String>,
    pub ok: bool,
    pub messages: Vec<String>,
    pub agent_version: String,
    pub hostname: String,
    #[serde(default)]
    pub os: String,
}

// ---------------------------------------------------------------------------
// Keys and signatures
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct KeyError(pub String);

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for KeyError {}

/// An ed25519 identity (server or device).
pub struct Identity(SigningKey);

impl Identity {
    pub fn generate() -> Result<Self, KeyError> {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|e| KeyError(format!("random: {e}")))?;
        Ok(Identity(SigningKey::from_bytes(&seed)))
    }
    pub fn from_seed(seed: &[u8]) -> Result<Self, KeyError> {
        let seed: [u8; 32] = seed.try_into().map_err(|_| KeyError("key file must be 32 bytes".into()))?;
        Ok(Identity(SigningKey::from_bytes(&seed)))
    }
    pub fn seed(&self) -> [u8; 32] {
        self.0.to_bytes()
    }
    pub fn public_key(&self) -> String {
        B64.encode(self.0.verifying_key().to_bytes())
    }
    pub fn sign(&self, msg: &[u8]) -> String {
        B64.encode(self.0.sign(msg).to_bytes())
    }
}

/// Verify a base64 signature against a base64 public key.
pub fn verify(public_key: &str, msg: &[u8], signature: &str) -> Result<(), KeyError> {
    let pk: [u8; 32] = B64
        .decode(public_key)
        .map_err(|_| KeyError("bad public key encoding".into()))?
        .try_into()
        .map_err(|_| KeyError("bad public key length".into()))?;
    let sig: [u8; 64] = B64
        .decode(signature)
        .map_err(|_| KeyError("bad signature encoding".into()))?
        .try_into()
        .map_err(|_| KeyError("bad signature length".into()))?;
    let vk = VerifyingKey::from_bytes(&pk).map_err(|_| KeyError("invalid public key".into()))?;
    vk.verify_strict(msg, &Signature::from_bytes(&sig))
        .map_err(|_| KeyError("signature mismatch".into()))
}

/// The exact bytes an agent signs for a request.
pub fn request_message(method: &str, path: &str, time: i64, body: &[u8]) -> Vec<u8> {
    format!("{method}\n{path}\n{time}\n{}", hex(&Sha256::digest(body))).into_bytes()
}

impl SignedManifest {
    pub fn sign(manifest: &Manifest, key: &Identity) -> Self {
        let payload = serde_json::to_string(manifest).expect("manifest serializes");
        let signature = key.sign(payload.as_bytes());
        SignedManifest { payload, signature }
    }
    /// Check the signature with the pinned server key, then parse.
    pub fn open(&self, server_key: &str) -> Result<Manifest, KeyError> {
        verify(server_key, self.payload.as_bytes(), &self.signature)?;
        serde_json::from_str(&self.payload).map_err(|e| KeyError(format!("manifest json: {e}")))
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_override_in_order() {
        let global = Settings { keyboard: Some("de".into()), ..Default::default() };
        let role = Settings { allow_app_install: Some(true), ..Default::default() };
        let device = Settings { keyboard: Some("ch:fr".into()), ..Default::default() };
        let s = Settings::resolve([&global, &role, &device]);
        assert_eq!(s.keyboard.as_deref(), Some("ch:fr"));
        assert_eq!(s.allow_app_install, Some(true));
        assert_eq!(s.locale.as_deref(), Some("de_CH.UTF-8")); // default kept
        assert_eq!(s.keyboard_parts(), ("ch".into(), "fr".into()));
    }

    #[test]
    fn partial_settings_serialize_compactly() {
        let s = Settings { timezone: Some("UTC".into()), ..Default::default() };
        assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"timezone":"UTC"}"#);
    }

    #[test]
    fn manifest_signature_roundtrip_and_tamper() {
        let key = Identity::generate().unwrap();
        let mut m = Manifest {
            revision: String::new(),
            device_name: "pc-01".into(),
            role: "student".into(),
            group: None,
            settings: Settings::defaults(),
            admin_password_hash: None,
            apps: vec![],
            issued_at: 1,
        };
        m.compute_revision();
        let rev = m.revision.clone();
        m.issued_at = 2;
        m.compute_revision();
        assert_eq!(rev, m.revision, "revision ignores issued_at");

        let signed = SignedManifest::sign(&m, &key);
        assert_eq!(signed.open(&key.public_key()).unwrap(), m);

        let mut bad = signed.clone();
        bad.payload = bad.payload.replace("student", "teacher");
        assert!(bad.open(&key.public_key()).is_err());

        let other = Identity::generate().unwrap();
        assert!(signed.open(&other.public_key()).is_err());
    }

    #[test]
    fn request_signature() {
        let key = Identity::generate().unwrap();
        let msg = request_message("GET", "/api/v1/manifest", 10, b"");
        let sig = key.sign(&msg);
        assert!(verify(&key.public_key(), &msg, &sig).is_ok());
        let other = request_message("GET", "/api/v1/manifest", 11, b"");
        assert!(verify(&key.public_key(), &other, &sig).is_err());
    }
}
