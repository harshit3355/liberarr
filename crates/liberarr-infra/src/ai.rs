//! OpenAI-compatible HTTP client: transcription and chat completions against any base URL.
//! Errors carry the HTTP status and a short provider message, never bodies, audio, text or the key.
use liberarr_core::summaries::{ChatMessage, Llm};
use liberarr_core::transcription::{ChunkResult, Segment, Transcriber};
use liberarr_core::{Error, Result};
use reqwest::multipart::{Form, Part};
use serde_json::{Value, json};
use std::time::Duration;

const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(600);
const CHAT_TIMEOUT: Duration = Duration::from_secs(180);

/// One profile (base URL, key, model). Build one for `transcribe` and one for `llm`.
pub struct OpenAiCompat {
    base_url: String,
    api_key: String,
    model: String,
    http: reqwest::Client,
}

impl OpenAiCompat {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            http: reqwest::Client::new(),
        }
    }

    /// Sends the request, maps failures to `Error::Provider` and returns the parsed JSON reply.
    async fn send(
        &self,
        path: &str,
        timeout: Duration,
        req: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
    ) -> Result<Value> {
        let url = format!("{}/{path}", self.base_url);
        let res = req(self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .timeout(timeout))
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
        if !status.is_success() {
            let msg = json
                .as_ref()
                .and_then(|j| j.pointer("/error/message")?.as_str())
                .map(|m| format!(": {}", m.chars().take(200).collect::<String>()))
                .unwrap_or_default();
            return Err(Error::Provider(format!("HTTP {status}{msg}")));
        }
        json.ok_or_else(|| Error::Provider(format!("HTTP {status}: reply is not JSON")))
    }
}

fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key)?.as_f64()
}

impl Transcriber for OpenAiCompat {
    async fn transcribe(&self, audio: &[u8], filename: &str) -> Result<ChunkResult> {
        // No `language`: recordings mix French and English.
        let form = Form::new()
            .text("model", self.model.clone())
            .text("response_format", "verbose_json")
            .part(
                "file",
                Part::bytes(audio.to_vec()).file_name(filename.to_string()),
            );
        let r = self
            .send("audio/transcriptions", TRANSCRIBE_TIMEOUT, |b| {
                b.multipart(form)
            })
            .await?;
        let text = r
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Provider("reply has no `text`".into()))?;
        // Segments are optional: some providers return plain text only.
        let segments = r
            .get("segments")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|s| {
                        Some(Segment {
                            start: num(s, "start")?,
                            end: num(s, "end")?,
                            text: s.get("text")?.as_str()?.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(ChunkResult {
            language: r.get("language").and_then(Value::as_str).map(String::from),
            text: text.to_string(),
            segments,
        })
    }

    fn engine(&self) -> &str {
        &self.model
    }
}

impl Llm for OpenAiCompat {
    async fn complete_json(&self, messages: &[ChatMessage]) -> Result<String> {
        let body = json!({
            "model": self.model,
            "messages": messages.iter().map(|m| json!({"role": m.role.as_str(), "content": m.content})).collect::<Vec<_>>(),
            "response_format": {"type": "json_object"},
        });
        let r = self
            .send("chat/completions", CHAT_TIMEOUT, |b| {
                b.header("content-type", "application/json")
                    .body(body.to_string())
            })
            .await?;
        r.pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(String::from)
            .ok_or_else(|| Error::Provider("reply has no message content".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, extract::Request, http::StatusCode, routing::post};
    use liberarr_core::Error;
    use liberarr_core::transcription::Transcriber;

    const KEY: &str = "sk-test-secret";

    /// Fails (400) unless the request is the documented multipart shape: bearer key, verbose_json, no language.
    async fn transcription(req: Request, reply: &'static str) -> (StatusCode, String) {
        let auth = req.headers().get("authorization").cloned();
        let body = axum::body::to_bytes(req.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8_lossy(&body);
        let ok = auth.is_some_and(|a| a == format!("Bearer {KEY}").as_str())
            && body.contains("name=\"model\"")
            && body.contains("verbose_json")
            && body.contains("name=\"file\"; filename=\"chunk-0.ogg\"")
            && !body.contains("name=\"language\"");
        if ok {
            (StatusCode::OK, reply.into())
        } else {
            (StatusCode::BAD_REQUEST, "bad request".into())
        }
    }

    async fn serve() -> String {
        let app = Router::new()
            .route(
                "/seg/audio/transcriptions",
                post(|req: Request| {
                    transcription(
                        req,
                        r#"{"language":"french","text":" Bonjour. Hello. ","segments":[{"start":0.0,"end":1.5,"text":" Bonjour."},{"start":1.5,"end":3.0,"text":" Hello."}]}"#,
                    )
                }),
            )
            .route(
                "/plain/audio/transcriptions",
                post(|req: Request| transcription(req, r#"{"text":"Bonjour"}"#)),
            )
            .route(
                "/denied/audio/transcriptions",
                post(|| async { (StatusCode::UNAUTHORIZED, r#"{"error":{"message":"Incorrect API key"}}"#) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn transcribe_parses_verbose_json_and_missing_segments() {
        let base = serve().await;
        let client =
            |p: &str| OpenAiCompat::new(format!("{base}/{p}/"), KEY.into(), "whisper-1".into());

        let r = client("seg")
            .transcribe(b"audio", "chunk-0.ogg")
            .await
            .unwrap();
        assert_eq!(r.language.as_deref(), Some("french"));
        assert_eq!(r.text, " Bonjour. Hello. ");
        assert_eq!(r.segments.len(), 2);
        assert_eq!((r.segments[1].start, r.segments[1].end), (1.5, 3.0));

        let r = client("plain")
            .transcribe(b"audio", "chunk-0.ogg")
            .await
            .unwrap();
        assert_eq!(
            (r.language, r.text.as_str(), r.segments.len()),
            (None, "Bonjour", 0)
        );

        let err = client("denied")
            .transcribe(b"audio", "chunk-0.ogg")
            .await
            .unwrap_err();
        assert!(
            matches!(&err, Error::Provider(m) if m.contains("401") && m.contains("Incorrect API key"))
        );
        assert!(!err.to_string().contains(KEY));
        assert_eq!(client("seg").engine(), "whisper-1");
    }
}
