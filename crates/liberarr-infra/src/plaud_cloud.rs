//! Plaud cloud client for `link` (spec §6.2): device list, serial signatures, key pair.
//! The token is only ever sent as a bearer header: it is never stored, logged or put in an error,
//! and neither is any reply body.
use aes::Aes128;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockModeDecrypt, KeyIvInit};
use hmac::{Hmac, KeyInit, Mac};
use liberarr_core::device::crypto::private_key_from_pem;
use liberarr_core::device::link::PlaudAccount;
use liberarr_core::{Error, Result};
use serde_json::{Value, json};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

const HOST: &str = "https://api.plaud.ai";
/// Built into the Plaud Android app; the gateway Fernet-encrypts replies with it.
const APP_FERNET_KEY: &str = "8rRmMCskL3NaUiGr-UsmEEeI8NKH-ZfeGbpRGqSLpuI=";
const TIMEOUT: Duration = Duration::from_secs(30);

fn err(msg: &str) -> Error {
    Error::Provider(format!("plaud cloud: {msg}"))
}

pub struct PlaudCloud {
    /// Starts at `HOST`, then the regional host once the gateway redirected us to it.
    base: Mutex<String>,
    http: reqwest::Client,
}

impl PlaudCloud {
    pub fn new() -> Self {
        Self::with_client(HOST, reqwest::Client::new())
    }

    fn with_client(base: &str, http: reqwest::Client) -> Self {
        Self {
            base: Mutex::new(base.to_string()),
            http,
        }
    }

    fn base(&self) -> String {
        self.base.lock().map(|b| b.clone()).unwrap_or_default()
    }

    /// One call, following a single `-302` region redirect. Returns the decoded JSON reply
    /// once its `status` is 0.
    async fn call(
        &self,
        token: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        for _ in 0..2 {
            let base = self.base();
            let reply = self
                .send(
                    token,
                    method.clone(),
                    &format!("{base}{path}"),
                    body.as_ref(),
                )
                .await?;
            match reply
                .get("status")
                .or_else(|| reply.get("code"))
                .and_then(Value::as_i64)
            {
                None | Some(0) => return Ok(reply),
                Some(-302) => {
                    let scheme = base.split_once("://").map_or("https", |(s, _)| s);
                    let host = region_host(&reply.to_string())
                        .ok_or_else(|| err("region redirect without a Plaud host"))?;
                    if let Ok(mut b) = self.base.lock() {
                        *b = format!("{scheme}://{host}");
                    }
                }
                Some(status) => return Err(err(&format!("request refused (status {status})"))),
            }
        }
        Err(err("too many region redirects"))
    }

    async fn send(
        &self,
        token: &str,
        method: reqwest::Method,
        url: &str,
        body: Option<&Value>,
    ) -> Result<Value> {
        let mut req = self
            .http
            .request(method, url)
            .bearer_auth(token)
            .header("X-Encrypt-Response", "1")
            .header("app-platform", "android")
            .timeout(TIMEOUT);
        if let Some(b) = body {
            req = req
                .header("Content-Type", "application/json")
                .body(b.to_string());
        }
        let res = req
            .send()
            .await
            .map_err(|e| err(&format!("request failed: {}", e.without_url())))?;
        let http = res.status();
        let encrypted = res
            .headers()
            .get("x-encrypt-response")
            .is_some_and(|v| v == "1");
        let text = res.text().await.map_err(|_| err("unreadable reply"))?;
        let json = if encrypted {
            fernet_decrypt(text.trim()).and_then(|p| serde_json::from_slice(&p).ok())
        } else {
            serde_json::from_str(&text).ok()
        };
        json.ok_or_else(|| err(&format!("HTTP {http}: unreadable reply")))
    }
}

impl Default for PlaudCloud {
    fn default() -> Self {
        Self::new()
    }
}

/// The first `https://<host>` in `text` whose host is a Plaud API host. The token is sent to
/// whatever this returns, so nothing else is accepted.
fn region_host(text: &str) -> Option<String> {
    text.match_indices("https://").find_map(|(i, m)| {
        let rest = &text[i + m.len()..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'))
            .unwrap_or(rest.len());
        let host = &rest[..end];
        let plaud = host.ends_with(".plaud.ai") || host.ends_with(".plaud.cn");
        (host.starts_with("api") && plaud).then(|| host.to_string())
    })
}

/// Fernet: `0x80 ‖ timestamp[8] ‖ iv[16] ‖ AES-128-CBC ciphertext ‖ HMAC-SHA256[32]`, base64url.
/// The timestamp is not checked (the gateway sets no TTL).
fn fernet_decrypt(token: &str) -> Option<Vec<u8>> {
    let key = URL_SAFE.decode(APP_FERNET_KEY).ok()?;
    let raw = URL_SAFE.decode(token).ok()?;
    let (msg, tag) = raw.split_at_checked(raw.len().checked_sub(32)?)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&key[..16]).ok()?;
    mac.update(msg);
    mac.verify_slice(tag).ok()?;
    let (head, ct) = msg.split_at_checked(25)?;
    (head[0] == 0x80).then_some(())?;
    cbc::Decryptor::<Aes128>::new_from_slices(&key[16..], &head[9..])
        .ok()?
        .decrypt_padded_vec::<Pkcs7>(ct)
        .ok()
}

impl PlaudAccount for PlaudCloud {
    async fn devices(&self, token: &str) -> Result<Vec<String>> {
        let b = self
            .call(token, reqwest::Method::GET, "/device-app/device/list", None)
            .await?;
        let mut list = b
            .get("data_devices")
            .or_else(|| b.pointer("/data/data_devices"))
            .or_else(|| b.get("data"))
            .unwrap_or(&Value::Null);
        if let Some(inner) = list.get("list").or_else(|| list.get("devices")) {
            list = inner;
        }
        Ok(list
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d.get("sn").or_else(|| d.get("serial_number"))?.as_str())
            .map(str::to_string)
            .collect())
    }

    async fn sign(&self, token: &str, serials: &[String]) -> Result<HashMap<String, Vec<u8>>> {
        let b = self
            .call(
                token,
                reqwest::Method::POST,
                "/device-app/device/sn-sign",
                Some(json!({ "sns": serials })),
            )
            .await?;
        let sigs = b
            .pointer("/data/signature")
            .and_then(Value::as_object)
            .ok_or_else(|| err("no signature in the reply"))?;
        sigs.iter()
            .map(|(sn, v)| {
                let sig = v.as_str().and_then(|s| STANDARD.decode(s).ok());
                Some((sn.clone(), sig?))
            })
            .collect::<Option<_>>()
            .ok_or_else(|| err("a signature is not base64"))
    }

    async fn key_pair(&self, token: &str) -> Result<(String, String)> {
        let b = self
            .call(
                token,
                reqwest::Method::POST,
                "/device-app/e2ee/gen_key",
                Some(json!({})),
            )
            .await?;
        let pem = |k: &str| {
            let raw = STANDARD
                .decode(b.pointer(&format!("/data/{k}"))?.as_str()?)
                .ok()?;
            String::from_utf8(raw).ok()
        };
        match (pem("private_key_base64"), pem("public_key_base64")) {
            (Some(private), Some(public))
                if private_key_from_pem(&private).is_ok() && public.contains("PUBLIC KEY") =>
            {
                Ok((private, public))
            }
            _ => Err(err("the reply is not a PEM key pair")),
        }
    }

    /// The JWT `sub` claim, read without verifying the signature (the server does that).
    async fn user_id(&self, token: &str) -> Result<String> {
        let claims = token
            .split('.')
            .nth(1)
            .and_then(|p| URL_SAFE_NO_PAD.decode(p.trim_end_matches('=')).ok())
            .and_then(|c| serde_json::from_slice::<Value>(&c).ok());
        claims
            .as_ref()
            .and_then(|c| c.get("sub")?.as_str())
            .map(str::to_string)
            .ok_or_else(|| err("the token has no `sub` claim"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::Aes128;
    use axum::Router;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::get;
    use cbc::cipher::block_padding::Pkcs7;
    use cbc::cipher::{BlockModeEncrypt, KeyIvInit};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A Fernet token as the Plaud app's gateway would produce it (fixed IV and timestamp).
    fn fernet_encrypt(plain: &[u8]) -> String {
        let key = URL_SAFE.decode(APP_FERNET_KEY).unwrap();
        let iv = [7u8; 16];
        let ct = cbc::Encryptor::<Aes128>::new_from_slices(&key[16..], &iv)
            .unwrap()
            .encrypt_padded_vec::<Pkcs7>(plain);
        let mut msg = vec![0x80, 0, 0, 0, 0, 0, 0, 0, 1];
        msg.extend_from_slice(&iv);
        msg.extend_from_slice(&ct);
        let mut mac = Hmac::<Sha256>::new_from_slice(&key[..16]).unwrap();
        mac.update(&msg);
        msg.extend_from_slice(&mac.finalize().into_bytes());
        URL_SAFE.encode(msg)
    }

    #[derive(Default)]
    struct Hits {
        global: AtomicU32,
        region: AtomicU32,
    }

    /// `api.plaud.ai` answers with the region redirect, `api-euc1.plaud.ai` with a Fernet body.
    /// Both names resolve to this one server and are told apart by the `Host` header.
    async fn list(State(hits): State<Arc<Hits>>, h: HeaderMap) -> impl IntoResponse {
        let host = h["host"].to_str().unwrap();
        let hdr = |n: &str| h.get(n).and_then(|v| v.to_str().ok()).unwrap_or("");
        if hdr("authorization") != "Bearer tok"
            || hdr("x-encrypt-response") != "1"
            || hdr("app-platform") != "android"
        {
            return (StatusCode::UNAUTHORIZED, HeaderMap::new(), String::new());
        }
        if host.starts_with("api-euc1.") {
            hits.region.fetch_add(1, Ordering::SeqCst);
            let mut out = HeaderMap::new();
            out.insert("x-encrypt-response", "1".parse().unwrap());
            let body = br#"{"status":0,"data_devices":[{"sn":"8810b50319526726"}]}"#;
            return (StatusCode::OK, out, fernet_encrypt(body) + "\n");
        }
        hits.global.fetch_add(1, Ordering::SeqCst);
        let body = r#"{"status":-302,"msg":"region","data":{"domains":{"api":"https://api-euc1.plaud.ai"}}}"#;
        (StatusCode::OK, HeaderMap::new(), body.into())
    }

    #[tokio::test]
    async fn cloud_follows_region_redirect_and_decrypts_fernet() {
        let hits = Arc::new(Hits::default());
        let app = Router::new()
            .route("/device-app/device/list", get(list))
            .with_state(hits.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });

        let http = reqwest::Client::builder()
            .resolve("api.plaud.ai", addr)
            .resolve("api-euc1.plaud.ai", addr)
            .build()
            .unwrap();
        let cloud = PlaudCloud::with_client("http://api.plaud.ai", http);

        assert_eq!(cloud.devices("tok").await.unwrap(), ["8810b50319526726"]);
        // The region is remembered: the second call goes straight to it.
        assert_eq!(cloud.devices("tok").await.unwrap().len(), 1);
        assert_eq!(hits.global.load(Ordering::SeqCst), 1);
        assert_eq!(hits.region.load(Ordering::SeqCst), 2);

        // A wrong token is an error that does not echo the token.
        let e = cloud
            .devices("wrong-token-value")
            .await
            .unwrap_err()
            .to_string();
        assert!(!e.contains("wrong-token-value"), "{e}");
    }
}
