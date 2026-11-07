//! Mistral transcription: multipart upload, segment timestamps. Chat needs no adapter here (it is
//! OpenAI-shaped, so `OpenAiCompat` serves the `Llm` side).
use crate::ai::chunk_result;
use crate::http::{self, Auth};
use liberarr_core::Result;
use liberarr_core::transcription::{ChunkResult, Transcriber};
use reqwest::multipart::{Form, Part};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(600);

pub struct MistralTranscriber {
    base_url: String,
    api_key: String,
    model: String,
    http: reqwest::Client,
}

impl MistralTranscriber {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            http: http::client(),
        }
    }
}

impl Transcriber for MistralTranscriber {
    async fn transcribe(&self, audio: &[u8], filename: &str) -> Result<ChunkResult> {
        // No `language`: recordings mix French and English (and Mistral drops timestamps with it).
        let form = Form::new()
            .text("model", self.model.clone())
            .text("timestamp_granularities", "segment")
            .part(
                "file",
                Part::bytes(audio.to_vec()).file_name(filename.to_string()),
            );
        let url = format!("{}/audio/transcriptions", self.base_url);
        let r = http::send(
            &self.http,
            &url,
            Auth::Bearer(&self.api_key),
            TIMEOUT,
            |b| b.multipart(form),
        )
        .await?;
        chunk_result(&r)
    }

    fn engine(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, extract::Request, http::StatusCode, routing::post};

    const KEY: &str = "sk-secret";

    async fn serve() -> String {
        let app = Router::new().route(
            "/seg/audio/transcriptions",
            post(|req: Request| async move {
                let body = axum::body::to_bytes(req.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let body = String::from_utf8_lossy(&body);
                let ok = body.contains("name=\"timestamp_granularities\"\r\n\r\nsegment\r\n")
                    && body.contains("name=\"model\"")
                    && !body.contains("name=\"language\"")
                    && !body.contains("name=\"response_format\"");
                if ok {
                    (
                        StatusCode::OK,
                        r#"{"text":"salut","segments":[{"start":0.0,"end":1.5,"text":"salut"}]}"#,
                    )
                } else {
                    (StatusCode::BAD_REQUEST, "bad request")
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn client(base: &str, p: &str) -> MistralTranscriber {
        MistralTranscriber::new(
            format!("{base}/{p}"),
            KEY.into(),
            "voxtral-mini-latest".into(),
        )
    }

    #[tokio::test]
    async fn parses_segments_and_sends_segment_granularity() {
        let base = serve().await;
        let r = client(&base, "seg")
            .transcribe(b"audio", "chunk-0.ogg")
            .await
            .unwrap();
        assert_eq!(r.segments.len(), 1);
        assert_eq!(r.segments[0].end, 1.5);
    }
}
