//! Signed HTTP client for the server API.

use std::time::Duration;
use valo_proto::{AGENT_HEADER_KEY, AGENT_HEADER_SIG, AGENT_HEADER_TIME, Identity, now_unix, request_message};

pub struct Client<'a> {
    base: String,
    key: &'a Identity,
    http: ureq::Agent,
}

pub struct Response {
    pub status: u16,
    pub body: String,
}

impl<'a> Client<'a> {
    pub fn new(base: &str, key: &'a Identity) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build();
        Client { base: base.trim_end_matches('/').to_string(), key, http: config.into() }
    }

    fn signature(&self, method: &str, path: &str, body: &[u8]) -> (String, String) {
        let t = now_unix();
        (t.to_string(), self.key.sign(&request_message(method, path, t, body)))
    }

    pub fn get(&self, path: &str) -> Result<Response, String> {
        let (t, sig) = self.signature("GET", path, b"");
        let mut r = self
            .http
            .get(format!("{}{path}", self.base))
            .header(AGENT_HEADER_KEY, self.key.public_key())
            .header(AGENT_HEADER_TIME, t)
            .header(AGENT_HEADER_SIG, sig)
            .call()
            .map_err(|e| format!("server not reachable: {e}"))?;
        let status = r.status().as_u16();
        let body = r.body_mut().read_to_string().map_err(|e| e.to_string())?;
        Ok(Response { status, body })
    }

    pub fn post_json<T: serde::Serialize>(&self, path: &str, value: &T) -> Result<Response, String> {
        let body = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        let (t, sig) = self.signature("POST", path, &body);
        let mut r = self
            .http
            .post(format!("{}{path}", self.base))
            .header(AGENT_HEADER_KEY, self.key.public_key())
            .header(AGENT_HEADER_TIME, t)
            .header(AGENT_HEADER_SIG, sig)
            .header("content-type", "application/json")
            .send(&body[..])
            .map_err(|e| format!("server not reachable: {e}"))?;
        let status = r.status().as_u16();
        let body = r.body_mut().read_to_string().map_err(|e| e.to_string())?;
        Ok(Response { status, body })
    }
}
