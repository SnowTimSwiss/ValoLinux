//! Small helpers: random values, form parsing, time formatting.

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("system random source");
    b
}

pub fn random_token() -> String {
    valo_proto::hex(&random_bytes::<32>())
}

/// Characters that cannot be confused when read from a screen (no 0/O, 1/I/L).
const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

fn random_from_alphabet(len: usize) -> String {
    let bytes = random_bytes::<64>();
    // 64 % 31 bias is negligible for codes that are confirmed by a human anyway
    bytes.iter().take(len).map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect()
}

/// Adoption code shown on the device: 6 characters, e.g. "K7M2QX".
pub fn adoption_code() -> String {
    random_from_alphabet(6)
}

/// Normalise what an admin typed: "k7m-2qx " -> "K7M2QX".
pub fn normalize_code(input: &str) -> String {
    input.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

pub fn format_code(code: &str) -> String {
    if code.len() == 6 { format!("{}-{}", &code[..3], &code[3..]) } else { code.to_string() }
}

/// Per-device local admin password (LAPS-like): 4 groups of 4, e.g. "K7M2-QXA9-...".
pub fn admin_password() -> String {
    let s = random_from_alphabet(16);
    format!("{}-{}-{}-{}", &s[0..4], &s[4..8], &s[8..12], &s[12..16])
}

/// crypt(3) SHA-512 hash usable with `usermod -p`.
pub fn crypt_sha512(password: &str) -> String {
    crypt_sha512_with_salt(password, &random_bytes::<12>())
}

/// SHA-512-crypt uses at most 16 salt characters. sha-crypt's default salt is
/// longer and gets truncated inside the hash but not in the string, so glibc
/// would produce a different string and logins fail. 12 bytes = exactly 16 chars.
fn crypt_sha512_with_salt(password: &str, salt: &[u8; 12]) -> String {
    use sha_crypt::{PasswordHasher, ShaCrypt};
    ShaCrypt::default().hash_password_with_salt(password.as_bytes(), salt).expect("sha-crypt").to_string()
}

/// Parsed `application/x-www-form-urlencoded` body (keeps repeated keys).
pub struct FormData(Vec<(String, String)>);

impl FormData {
    pub fn parse(body: &str) -> Self {
        FormData(form_urlencoded::parse(body.as_bytes()).into_owned().collect())
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    pub fn text(&self, key: &str) -> String {
        self.get(key).unwrap_or("").trim().to_string()
    }
    pub fn all(&self, key: &str) -> Vec<String> {
        self.0.iter().filter(|(k, _)| k == key).map(|(_, v)| v.clone()).collect()
    }
}

pub fn url_encode(s: &str) -> String {
    form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// "gerade eben", "vor 5 Min.", "vor 3 Std.", "vor 2 Tagen"
pub fn ago(ts: Option<i64>) -> String {
    let Some(ts) = ts else { return "nie".into() };
    let d = (valo_proto::now_unix() - ts).max(0);
    match d {
        0..60 => "gerade eben".into(),
        60..3600 => format!("vor {} Min.", d / 60),
        3600..86400 => format!("vor {} Std.", d / 3600),
        _ => format!("vor {} Tagen", d / 86400),
    }
}

/// A device counts as online if it was seen within the last 10 minutes.
pub fn is_online(ts: Option<i64>) -> bool {
    ts.is_some_and(|t| valo_proto::now_unix() - t < 600)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_passwords() {
        let c = adoption_code();
        assert_eq!(c.len(), 6);
        assert!(c.bytes().all(|b| ALPHABET.contains(&b)));
        assert_eq!(normalize_code(" k7m-2qx "), "K7M2QX");
        assert_eq!(format_code("K7M2QX"), "K7M-2QX");
        let p = admin_password();
        assert_eq!(p.len(), 19);
        let h = crypt_sha512(&p);
        assert!(h.starts_with("$6$"));
        let salt = h.split('$').nth(3).unwrap();
        assert_eq!(salt.len(), 16, "{h}");
    }

    #[test]
    fn crypt_matches_glibc() {
        // reference: `openssl passwd -6 -salt '.2U.1EE/4Q.07ck0' Test-1234` (identical to glibc crypt)
        let h = crypt_sha512_with_salt("Test-1234", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        assert_eq!(h.replacen("rounds=5000$", "", 1), "$6$.2U.1EE/4Q.07ck0$1Enw.Y8y5cg05yy4TQ.0IdC1Wvg550qmJzY2XwZlfr9BgfSOh4litFDhQZSdTL1yM66xX4BcDNOlBZKiD6MTi0");
    }

    #[test]
    fn form_keeps_repeated_keys() {
        let f = FormData::parse("apps=gimp&apps=vlc&name=PC+01");
        assert_eq!(f.all("apps"), vec!["gimp", "vlc"]);
        assert_eq!(f.text("name"), "PC 01");
    }
}
