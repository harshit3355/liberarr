//! Claude summaries over the Messages API (system prompt apart, text blocks in the reply).
use crate::http::{self, Auth};
use liberarr_core::summaries::{ChatMessage, Llm, Role};
use liberarr_core::{Error, Result};
use serde_json::{Value, json};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(180);
pub(crate) const API_VERSION: &str = "2026-10-21";
/// Room for a long meeting summary; hitting it is an error, not a truncated reply.
const MAX_TOKENS: u32 = 8192;

pub struct AnthropicLlm {
    base_url: String,
    api_key: String,
    model: String,
    http: reqwest::Client,
}

impl AnthropicLlm {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            http: http::client(),
        }
    }
}

impl Llm for AnthropicLlm {
    async fn complete_json(&self, messages: &[ChatMessage]) -> Result<String> {
        let (system, rest): (Vec<_>, Vec<_>) =
            messages.iter().partition(|m| m.role == Role::System);
        // No sampling parameters, no prefill, no forced tool: recent models reject them.
        let mut body = json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "messages": rest.iter().map(|m| json!({"role": m.role.as_str(), "content": m.content})).collect::<Vec<_>>(),
        });
        if !system.is_empty() {
            let system: Vec<_> = system.iter().map(|m| m.content.as_str()).collect();
            body["system"] = system.join("\n").into();
        }
        let url = format!("{}/v1/messages", self.base_url);
        let r = http::send(
            &self.http,
            &url,
            Auth::Header("x-api-key", &self.api_key),
            TIMEOUT,
            true,
            |b| {
                b.header("anthropic-version", API_VERSION)
                    .header("content-type", "application/json")
                    .body(body.to_string())
            },
        )
        .await?;
        if r.get("stop_reason").and_then(Value::as_str) == Some("max_tokens") {
            return Err(Error::Provider("reply cut at max_tokens".into()));
        }
        r.get("content")
            .and_then(Value::as_array)
            .and_then(|a| {
                a.iter()
                    .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            })
            .and_then(|b| b.get("text")?.as_str())
            .map(String::from)
            .ok_or_else(|| Error::Provider("reply has no text content".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, extract::Request, http::StatusCode, routing::post};
    use liberarr_core::summaries::Role;
    use serde_json::{Value, json};

    const KEY: &str = "sk-ant-secret";

    async fn serve() -> String {
        let app = Router::new()
            .route(
                "/ok/v1/messages",
                post(|req: Request| async move {
                    let headers = req.headers().clone();
                    let body = axum::body::to_bytes(req.into_body(), usize::MAX)
                        .await
                        .unwrap();
                    let b: Value = serde_json::from_slice(&body).unwrap();
                    let ok = headers.get("anthropic-version").is_some_and(|v| v == "2026-10-21")
                        && headers.get("x-api-key").is_some_and(|v| v == KEY)
                        && b["system"] == "be brief"
                        && b["messages"] == json!([{"role": "user", "content": "hi"}])
                        && b.get("temperature").is_none();
                    if ok {
                        (
                            StatusCode::OK,
                            Json(json!({"content":[{"type":"text","text":"{\"a\":1}"}],"stop_reason":"end_turn"})),
                        )
                    } else {
                        (StatusCode::BAD_REQUEST, Json(json!({})))
                    }
                }),
            )
            .route(
                "/cut/v1/messages",
                post(|| async {
                    Json(json!({"content":[{"type":"text","text":"{\"a\":"}],"stop_reason":"max_tokens"}))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn client(base: &str, p: &str) -> AnthropicLlm {
        AnthropicLlm::new(format!("{base}/{p}"), KEY.into(), "claude-x".into())
    }

    fn messages() -> Vec<ChatMessage> {
        vec![
            ChatMessage {
                role: Role::System,
                content: "be brief".into(),
            },
            ChatMessage {
                role: Role::User,
                content: "hi".into(),
            },
        ]
    }

    #[tokio::test]
    async fn sends_version_header_and_splits_system() {
        let base = serve().await;
        let out = client(&base, "ok")
            .complete_json(&messages())
            .await
            .unwrap();
        assert_eq!(out, r#"{"a":1}"#);
    }

    #[tokio::test]
    async fn max_tokens_stop_is_an_error() {
        let base = serve().await;
        let err = client(&base, "cut")
            .complete_json(&messages())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("max_tokens"), "{err}");
    }
}
