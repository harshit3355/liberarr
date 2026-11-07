//! Shared POST for the AI adapters. Errors carry the HTTP status and a short provider message,
//! never bodies, audio, text or the key.
use liberarr_core::{Error, Result};
use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;

pub(crate) enum Auth<'a> {
    Bearer(&'a str),
    Header(&'static str, &'a str),
}

/// No redirects: a hostile or misconfigured base URL must not forward the key or the audio.
pub(crate) fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_default()
}

/// Sends the request, maps failures to `Error::Provider` and returns the parsed JSON reply.
pub(crate) async fn send(
    http: &reqwest::Client,
    url: &str,
    auth: Auth<'_>,
    timeout: Duration,
    req: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
) -> Result<Value> {
    let builder = http.post(url).timeout(timeout);
    let builder = match auth {
        Auth::Bearer(key) => builder.bearer_auth(key),
        Auth::Header(name, key) => builder.header(name, key),
    };
    let res = req(builder)
        .send()
        .await
        .map_err(|e| Error::Provider(format!("request failed: {}", e.without_url())))?;
    let status = res.status();
    let body = res.text().await.map_err(|e| {
        Error::Provider(format!(
            "HTTP {status}: unreadable reply: {}",
            e.without_url()
        ))
    })?;
    let json: Option<Value> = serde_json::from_str(&body).ok();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        // Providers quote a masked key in these messages: keep only the status.
        return Err(Error::Provider(format!(
            "provider refused the API key (HTTP {status})"
        )));
    }
    if !status.is_success() {
        let msg = json
            .as_ref()
            // `/error/message` (OpenAI, Anthropic) or a top-level `/message` (Mistral).
            .and_then(|j| {
                j.pointer("/error/message")
                    .or_else(|| j.pointer("/message"))?
                    .as_str()
            })
            .filter(|m| !m.to_lowercase().contains("key"))
            .map(|m| format!(": {}", m.chars().take(200).collect::<String>()))
            .unwrap_or_default();
        return Err(Error::Provider(format!("HTTP {status}{msg}")));
    }
    json.ok_or_else(|| Error::Provider(format!("HTTP {status}: reply is not JSON")))
}
