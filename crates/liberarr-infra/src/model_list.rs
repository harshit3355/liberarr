//! Lists the models an account can use: doubles as the connection test (no token spent).
//! Errors follow `http`: status and a short message, never a body or the key.
use crate::anthropic::API_VERSION;
use crate::http::{self, Auth};
use liberarr_core::ai::Provider;
use liberarr_core::{Error, Result};
use serde_json::Value;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(15);

/// `GET {base}/models` (bearer) or, for Anthropic, `GET {base}/v1/models`. Reads `data[].id`; any
/// other JSON shape gives an empty list. The `claude` CLI has no HTTP API.
pub async fn list_models(provider: Provider, base_url: &str, api_key: &str) -> Result<Vec<String>> {
    let base = base_url.trim_end_matches('/');
    let http = http::client();
    let reply = match provider {
        Provider::Claude => {
            return Err(Error::Provider("the claude CLI has no model list".into()));
        }
        // Without `limit`, Anthropic returns only the first 20 models.
        Provider::Anthropic => {
            http::get(
                &http,
                &format!("{base}/v1/models?limit=1000"),
                Auth::Header("x-api-key", api_key),
                TIMEOUT,
                |b| b.header("anthropic-version", API_VERSION),
            )
            .await?
        }
        _ => {
            http::get(
                &http,
                &format!("{base}/models"),
                Auth::Bearer(api_key),
                TIMEOUT,
                |b| b,
            )
            .await?
        }
    };
    Ok(reply
        .get("data")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("id")?.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, extract::Request, http::StatusCode, routing::get};

    const KEY: &str = "sk-test-secret";

    async fn serve() -> String {
        let app = Router::new()
            .route(
                "/ok/models",
                get(|req: Request| async move {
                    let ok = req
                        .headers()
                        .get("authorization")
                        .is_some_and(|a| a == format!("Bearer {KEY}").as_str());
                    if ok {
                        (
                            StatusCode::OK,
                            r#"{"data":[{"id":"a"},{"id":"b"},{"x":1}]}"#,
                        )
                    } else {
                        (StatusCode::BAD_REQUEST, "{}")
                    }
                }),
            )
            .route(
                "/anthropic/v1/models",
                get(|req: Request| async move {
                    let h = req.headers();
                    let ok = h.get("x-api-key").is_some_and(|v| v == KEY)
                        && h.get("anthropic-version").is_some_and(|v| v == API_VERSION);
                    if ok {
                        (StatusCode::OK, r#"{"data":[{"id":"claude-x"}]}"#)
                    } else {
                        (StatusCode::BAD_REQUEST, "{}")
                    }
                }),
            )
            .route(
                "/other/models",
                get(|| async { (StatusCode::OK, r#"{"models":["a"]}"#) }),
            )
            .route(
                "/denied/models",
                get(|| async {
                    (
                        StatusCode::UNAUTHORIZED,
                        r#"{"error":{"message":"Incorrect API key provided: sk-...abcd."}}"#,
                    )
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn lists_ids_and_tolerates_other_shapes() {
        let base = serve().await;
        let ids = list_models(Provider::Mistral, &format!("{base}/ok/"), KEY).await;
        assert_eq!(ids.unwrap(), ["a", "b"]);
        let ids = list_models(Provider::Anthropic, &format!("{base}/anthropic"), KEY).await;
        assert_eq!(ids.unwrap(), ["claude-x"]);
        let ids = list_models(Provider::OpenAi, &format!("{base}/other"), KEY).await;
        assert!(ids.unwrap().is_empty());
    }

    #[tokio::test]
    async fn refused_key_gives_the_fixed_text_without_the_key() {
        let base = serve().await;
        let err = list_models(Provider::OpenAi, &format!("{base}/denied"), KEY)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("provider refused the API key"), "{err}");
        assert!(!err.contains("sk-...abcd") && !err.contains(KEY), "{err}");
    }
}
