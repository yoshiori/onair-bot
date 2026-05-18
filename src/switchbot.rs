//! Minimal SwitchBot API v1.1 client.
//!
//! Implements just the device command endpoint needed to switch the on-air
//! light on and off, including the HMAC-SHA256 request signing the API
//! requires. See <https://github.com/OpenWonderLabs/SwitchBotAPI>.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

const API_BASE: &str = "https://api.switch-bot.com";
/// `statusCode` the API returns when a command succeeds.
const SUCCESS_STATUS_CODE: i64 = 100;
const MAX_ATTEMPTS: u32 = 3;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

type HmacSha256 = Hmac<Sha256>;

/// Compute the `sign` header value for a SwitchBot API v1.1 request.
///
/// The signature is the uppercased base64 encoding of
/// `HMAC-SHA256(secret, token + t + nonce)`.
pub fn sign(token: &str, secret: &str, t: &str, nonce: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts a key of any length");
    mac.update(token.as_bytes());
    mac.update(t.as_bytes());
    mac.update(nonce.as_bytes());
    let digest = mac.finalize().into_bytes();
    base64::engine::general_purpose::STANDARD
        .encode(digest)
        .to_uppercase()
}

/// A SwitchBot device that can be switched on and off.
pub struct SwitchBot {
    token: String,
    secret: String,
    device_id: String,
    agent: ureq::Agent,
}

impl SwitchBot {
    /// Create a client for a single device.
    pub fn new(token: String, secret: String, device_id: String) -> Self {
        let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
        Self {
            token,
            secret,
            device_id,
            agent,
        }
    }

    /// Switch the device on, retrying transient failures.
    pub fn turn_on(&self) -> Result<(), String> {
        self.send_command("turnOn")
    }

    /// Switch the device off, retrying transient failures.
    pub fn turn_off(&self) -> Result<(), String> {
        self.send_command("turnOff")
    }

    fn send_command(&self, command: &str) -> Result<(), String> {
        let mut last_error = String::new();
        for attempt in 1..=MAX_ATTEMPTS {
            match self.try_send_command(command) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    log::warn!(
                        "SwitchBot {command} attempt {attempt}/{MAX_ATTEMPTS} failed: {error}"
                    );
                    last_error = error;
                }
            }
        }
        Err(format!(
            "SwitchBot {command} failed after {MAX_ATTEMPTS} attempts: {last_error}"
        ))
    }

    fn try_send_command(&self, command: &str) -> Result<(), String> {
        let t = unix_millis();
        let nonce = uuid::Uuid::new_v4().to_string();
        let sign = sign(&self.token, &self.secret, &t, &nonce);
        let url = format!("{API_BASE}/v1.1/devices/{}/commands", self.device_id);

        let body = serde_json::json!({
            "command": command,
            "parameter": "default",
            "commandType": "command",
        });

        let response = self
            .agent
            .post(&url)
            .set("Authorization", &self.token)
            .set("sign", &sign)
            .set("t", &t)
            .set("nonce", &nonce)
            .set("Content-Type", "application/json; charset=utf8")
            .send_json(body)
            .map_err(|error| format!("HTTP request failed: {error}"))?;

        let json: serde_json::Value = response
            .into_json()
            .map_err(|error| format!("could not parse API response: {error}"))?;

        match json.get("statusCode").and_then(serde_json::Value::as_i64) {
            Some(SUCCESS_STATUS_CODE) => Ok(()),
            Some(code) => {
                let message = json
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("no message");
                Err(format!("API returned statusCode {code}: {message}"))
            }
            None => Err(format!("API response is missing statusCode: {json}")),
        }
    }
}

fn unix_millis() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_matches_known_vector() {
        // Reference value computed independently with:
        //   printf '%s' 'test-token1700000000000test-nonce' \
        //     | openssl dgst -sha256 -hmac 'test-secret' -binary | base64
        let signature = sign("test-token", "test-secret", "1700000000000", "test-nonce");
        assert_eq!(signature, "BQDXIXQGKZ4CKHQB4TTCI1Y0UKNZ9BXLW9EPD4M3RQE=");
    }

    #[test]
    fn sign_is_uppercase_and_deterministic() {
        let a = sign("tok", "sec", "1", "n");
        let b = sign("tok", "sec", "1", "n");
        assert_eq!(a, b);
        assert_eq!(a, a.to_uppercase());
    }

    #[test]
    fn sign_changes_with_every_input() {
        let base = sign("tok", "sec", "1", "n");
        assert_ne!(base, sign("TOK", "sec", "1", "n"));
        assert_ne!(base, sign("tok", "SEC", "1", "n"));
        assert_ne!(base, sign("tok", "sec", "2", "n"));
        assert_ne!(base, sign("tok", "sec", "1", "N"));
    }
}
