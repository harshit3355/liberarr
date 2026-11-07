//! Single-admin auth: signed session cookie or `X-Api-Key`, plus the login rate limit.
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;

pub const COOKIE: &str = "liberarr_session";
const SESSION_SECS: i64 = 14 * 24 * 3600;
const MAX_FAILURES: u32 = 5;
const WINDOW: Duration = Duration::from_secs(15 * 60);

/// Everything comes from `<LIBERARR_HOME>/.env`, read by the composition root.
pub struct AuthConfig {
    pub username: String,
    pub password: String,
    pub api_key: String,
    /// `LIBERARR_SECRET_KEY`: only a dedicated HKDF-derived key signs sessions.
    pub secret_key: String,
    pub cookie_secure: bool,
}

pub struct Auth {
    cfg: AuthConfig,
    session_key: [u8; 32],
    // shortcut: per-process and keyed on the peer address, so behind a reverse proxy all clients
    // share one budget; upgrade to a trusted X-Forwarded-For if that matters.
    failures: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}

/// Compares digests so neither content nor length leaks through timing.
fn ct_eq(a: &str, b: &str) -> bool {
    Sha256::digest(a).ct_eq(&Sha256::digest(b)).into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Auth {
    pub fn new(cfg: AuthConfig) -> Self {
        let mut session_key = [0u8; 32];
        hkdf::Hkdf::<Sha256>::new(None, cfg.secret_key.as_bytes())
            .expand(b"liberarr session v1", &mut session_key)
            .expect("32 bytes is a valid HKDF-SHA256 output length");
        Self {
            cfg,
            session_key,
            failures: Mutex::default(),
        }
    }

    fn tag(&self, expiry: i64) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.session_key).expect("any key length");
        mac.update(expiry.to_string().as_bytes());
        hex(&mac.finalize().into_bytes())
    }

    fn now() -> i64 {
        jiff::Timestamp::now().as_second()
    }

    fn cookie(&self, value: &str, max_age: i64) -> String {
        let secure = if self.cfg.cookie_secure {
            "; Secure"
        } else {
            ""
        };
        format!("{COOKIE}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}")
    }

    /// `Set-Cookie` value for a fresh 14-day session.
    pub fn session_cookie(&self) -> String {
        let expiry = Self::now() + SESSION_SECS;
        self.cookie(&format!("{expiry}.{}", self.tag(expiry)), SESSION_SECS)
    }

    pub fn clear_cookie(&self) -> String {
        self.cookie("", 0)
    }

    pub fn credentials_ok(&self, username: &str, password: &str) -> bool {
        // `&`, not `&&`: both are always compared.
        ct_eq(username, &self.cfg.username) & ct_eq(password, &self.cfg.password)
    }

    pub fn username(&self) -> &str {
        &self.cfg.username
    }

    /// A valid `X-Api-Key` header, or an unexpired, correctly signed session cookie.
    pub fn authorized(&self, api_key: Option<&str>, cookie_header: Option<&str>) -> bool {
        if let Some(k) = api_key
            && !self.cfg.api_key.is_empty()
            && ct_eq(k, &self.cfg.api_key)
        {
            return true;
        }
        let Some(value) = cookie_header.and_then(|h| {
            h.split(';')
                .filter_map(|c| c.trim().strip_prefix(COOKIE)?.strip_prefix('='))
                .next()
        }) else {
            return false;
        };
        let Some((expiry, tag)) = value.split_once('.') else {
            return false;
        };
        let Ok(expiry) = expiry.parse::<i64>() else {
            return false;
        };
        ct_eq(tag, &self.tag(expiry)) && expiry > Self::now()
    }

    pub fn is_blocked(&self, ip: IpAddr) -> bool {
        let map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&ip)
            .is_some_and(|(n, since)| *n >= MAX_FAILURES && since.elapsed() < WINDOW)
    }

    pub fn record_failure(&self, ip: IpAddr) {
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, (_, since)| since.elapsed() < WINDOW);
        map.entry(ip).or_insert((0, Instant::now())).0 += 1;
    }

    pub fn clear_failures(&self, ip: IpAddr) {
        self.failures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&ip);
    }
}
