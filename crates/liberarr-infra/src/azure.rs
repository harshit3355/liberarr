//! Azure AI Foundry / Azure OpenAI transcription. Audio only works on the deployment route
//! (`/openai/deployments/{deployment}/audio/transcriptions`); chat uses `OpenAiCompat` on the v1 URL.
use crate::ai::chunk_result;
use crate::http::{self, Auth};
use liberarr_core::transcription::{ChunkResult, Transcriber};
use liberarr_core::{Error, Result};
use reqwest::multipart::{Form, Part};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(600);
const API_VERSION: &str = "2025-04-01-preview";

pub struct AzureTranscriber {
    base_url: String,
    api_key: String,
    deployment: String,
    http: reqwest::Client,
}

impl AzureTranscriber {
    pub fn new(base_url: String, api_key: String, deployment: String) -> Self {
        Self {
            base_url,
            api_key,
            deployment,
            http: http::client(),
        }
    }

    /// The configured URL cut before its `/openai` segment, so `https://x.openai.azure.com` and
    /// `https://x.openai.azure.com/openai/v1/` both work.
    fn url(&self) -> Result<reqwest::Url> {
        let bad = || Error::Provider("base URL is not valid".into());
        let mut url = reqwest::Url::parse(&self.base_url).map_err(|_| bad())?;
        let mut path: Vec<String> = url
            .path_segments()
            .map(|s| {
                s.take_while(|s| *s != "openai")
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        path.extend(
            [
                "openai",
                "deployments",
                &self.deployment,
                "audio",
                "transcriptions",
            ]
            .map(String::from),
        );
        url.path_segments_mut()
            .map_err(|_| bad())?
            .clear()
            .extend(path);
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("api-version", API_VERSION);
        Ok(url)
    }
}

impl Transcriber for AzureTranscriber {
    async fn transcribe(&self, audio: &[u8], filename: &str) -> Result<ChunkResult> {
        // shortcut: deployment names are free-form, so whisper is guessed from the name; add an
        // explicit setting if someone names a whisper deployment otherwise.
        let format = if self.deployment.to_lowercase().contains("whisper") {
            "verbose_json"
        } else {
            "json"
        };
        // No `model` (the deployment is in the URL) and no `language`: recordings mix French and English.
        let form = Form::new().text("response_format", format).part(
            "file",
            Part::bytes(audio.to_vec()).file_name(filename.to_string()),
        );
        let url = self.url()?;
        let r = http::send(
            &self.http,
            url.as_str(),
            Auth::Bearer(&self.api_key),
            TIMEOUT,
            |b| b.multipart(form),
        )
        .await?;
        chunk_result(&r)
    }

    fn engine(&self) -> &str {
        &self.deployment
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, extract::Request, http::StatusCode, routing::post};

    #[tokio::test]
    async fn uses_the_deployment_route_and_reads_languages() {
        let app = Router::new().route(
            "/openai/deployments/dep/audio/transcriptions",
            post(|req: Request| async move {
                let query = req.uri().query().unwrap_or_default().to_string();
                let body = axum::body::to_bytes(req.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let body = String::from_utf8_lossy(&body);
                if query.contains("api-version=")
                    && body.contains("name=\"response_format\"\r\n\r\njson\r\n")
                {
                    (StatusCode::OK, r#"{"text":"salut","languages":["fr"]}"#)
                } else {
                    (StatusCode::BAD_REQUEST, "bad request")
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let t = AzureTranscriber::new(
            format!("http://{addr}/openai/v1/"),
            "k".into(),
            "dep".into(),
        );
        let r = t.transcribe(b"audio", "chunk-0.ogg").await.unwrap();
        assert_eq!(r.text, "salut");
        assert!(r.segments.is_empty());
        assert_eq!(r.language.as_deref(), Some("fr"));
    }
}
