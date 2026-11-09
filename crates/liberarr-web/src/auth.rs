//! Single-admin auth: signed session cookie or `X-Api-Key`, plus the login rate limit.
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;

pub const COOKIE: &str = "liberarr_session";
const SESSION_MS: i64 = 14 * 24 * 3600 * 1000;
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
    /// Part of every session MAC: changing the admin password revokes existing sessions.
    password_hash: [u8; 32],
    /// Sessions issued at or before this instant (ms) were logged out.
    // shortcut: in memory, so a restart forgets logouts until the 14-day expiry; persist it if
    // that matters.
    revoked_before: AtomicI64,
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
            password_hash: Sha256::digest(&cfg.password).into(),
            cfg,
            session_key,
            failures: Mutex::default(),
            revoked_before: AtomicI64::new(0),
        }
    }

    fn tag(&self, expiry: i64) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.session_key).expect("any key length");
        mac.update(expiry.to_string().as_bytes());
        mac.update(&self.password_hash);
        hex(&mac.finalize().into_bytes())
    }

    /// Milliseconds, so a logout and the next login rarely share a tick.
    fn now() -> i64 {
        jiff::Timestamp::now().as_millisecond()
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
        let expiry = Self::now() + SESSION_MS;
        self.cookie(&format!("{expiry}.{}", self.tag(expiry)), SESSION_MS / 1000)
    }

    /// Clears the cookie and revokes every session issued so far, including copies of it.
    pub fn logout(&self) -> String {
        self.revoked_before
            .fetch_max(Self::now(), Ordering::Relaxed);
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
        ct_eq(tag, &self.tag(expiry))
            && expiry > Self::now()
            && expiry - SESSION_MS > self.revoked_before.load(Ordering::Relaxed)
    }

    /// IPv6 peers share one budget per /64: SLAAC lets a host rotate addresses inside it.
    fn rate_key(ip: IpAddr) -> IpAddr {
        match ip.to_canonical() {
            IpAddr::V6(v6) => IpAddr::V6((v6.to_bits() & !u128::from(u64::MAX)).into()),
            v4 => v4,
        }
    }

    /// Counts one login attempt for `ip` and says whether it may proceed. Check and count share
    /// one lock, so a parallel burst cannot exceed the budget. A success calls `clear_failures`.
    pub fn begin_attempt(&self, ip: IpAddr) -> bool {
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, (_, since)| since.elapsed() < WINDOW);
        let entry = map.entry(Self::rate_key(ip)).or_insert((0, Instant::now()));
        if entry.0 >= MAX_FAILURES {
            return false;
        }
        entry.0 += 1;
        true
    }

    pub fn clear_failures(&self, ip: IpAddr) {
        self.failures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&Self::rate_key(ip));
    }
}
