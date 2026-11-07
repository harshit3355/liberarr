//! HTTP API and the embedded web UI.
//!
//! The core ports are native `async fn` traits whose futures are not provably `Send`, which axum
//! requires. `Backend` is the one seam: the same operations with `Send` futures, implemented by
//! the composition root where the concrete (SQLite) types make the bound provable.
mod api;
pub mod assets;
pub mod auth;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, patch, post};
use liberarr_core::Result;
use liberarr_core::jobs::JobInfo;
use liberarr_core::recordings::{Recording, RecordingItem};
use liberarr_core::summaries::MeetingSummary;
use liberarr_core::transcription::Transcript;
use std::future::Future;
use std::sync::Arc;

pub use auth::{Auth, AuthConfig};

pub trait Backend: Send + Sync + 'static {
    fn list(&self) -> impl Future<Output = Result<Vec<RecordingItem>>> + Send;
    fn recording(&self, id: i64) -> impl Future<Output = Result<Option<Recording>>> + Send;
    fn transcript(&self, id: i64) -> impl Future<Output = Result<Option<Transcript>>> + Send;
    fn summary(&self, id: i64) -> impl Future<Output = Result<Option<MeetingSummary>>> + Send;
    fn jobs(&self, id: i64) -> impl Future<Output = Result<Vec<JobInfo>>> + Send;
    /// Queues `step` (already validated against `STEPS`); the worker picks it up.
    fn enqueue(&self, id: i64, step: &str) -> impl Future<Output = Result<i64>> + Send;
    /// `Error::NotFound` when the summary or the action index does not exist.
    fn toggle_action(
        &self,
        id: i64,
        index: usize,
        done: bool,
    ) -> impl Future<Output = Result<MeetingSummary>> + Send;
}

pub struct AppState<B> {
    pub backend: Arc<B>,
    pub auth: Arc<Auth>,
}

impl<B> AppState<B> {
    pub fn new(backend: B, auth: AuthConfig) -> Self {
        Self {
            backend: Arc::new(backend),
            auth: Arc::new(Auth::new(auth)),
        }
    }
}

// Manual: `derive(Clone)` would demand `B: Clone`.
impl<B> Clone for AppState<B> {
    fn clone(&self) -> Self {
        Self {
            backend: Arc::clone(&self.backend),
            auth: Arc::clone(&self.auth),
        }
    }
}

/// Serve it with `into_make_service_with_connect_info::<SocketAddr>()` (login rate limit).
pub fn router<B: Backend>(state: AppState<B>) -> Router {
    let open = Router::new()
        .route("/health", get(api::health))
        .route("/auth/login", post(api::login::<B>))
        .route("/auth/logout", post(api::logout::<B>));
    let protected = Router::new()
        .route("/auth/me", get(api::me::<B>))
        .route("/recordings", get(api::list::<B>))
        .route("/recordings/{id}", get(api::detail::<B>))
        .route("/recordings/{id}/audio", get(api::audio::<B>))
        .route("/recordings/{id}/rerun", post(api::rerun::<B>))
        .route(
            "/recordings/{id}/actions/{index}",
            patch(api::toggle_action::<B>),
        )
        .layer(from_fn_with_state(state.clone(), api::require_auth::<B>));
    Router::new()
        .nest("/api", open.merge(protected).fallback(api::not_found))
        .fallback(get(assets::spa))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode, header};
    use liberarr_core::recordings::{RecordingStatus, Source};
    use std::net::SocketAddr;
    use tower::ServiceExt;

    struct Fake;
    impl Backend for Fake {
        async fn list(&self) -> Result<Vec<RecordingItem>> {
            Ok(vec![RecordingItem {
                id: 1,
                title: Some("t".into()),
                source: Source::File,
                duration_s: None,
                created_at: jiff::Timestamp::UNIX_EPOCH,
                status: RecordingStatus::Done,
                failed_step: None,
            }])
        }
        async fn recording(&self, _: i64) -> Result<Option<Recording>> {
            Ok(None)
        }
        async fn transcript(&self, _: i64) -> Result<Option<Transcript>> {
            Ok(None)
        }
        async fn summary(&self, _: i64) -> Result<Option<MeetingSummary>> {
            Ok(None)
        }
        async fn jobs(&self, _: i64) -> Result<Vec<JobInfo>> {
            Ok(vec![])
        }
        async fn enqueue(&self, _: i64, _: &str) -> Result<i64> {
            Ok(1)
        }
        async fn toggle_action(&self, _: i64, _: usize, _: bool) -> Result<MeetingSummary> {
            Err(liberarr_core::Error::NotFound("summary".into()))
        }
    }

    fn app() -> Router {
        router(AppState::new(
            Fake,
            AuthConfig {
                username: "admin".into(),
                password: "pw".into(),
                api_key: "key".into(),
                secret_key: "secret".into(),
                cookie_secure: false,
            },
        ))
    }

    fn req(method: &str, uri: &str, key: Option<&str>, body: &str) -> Request<Body> {
        let mut b = Request::builder().method(method).uri(uri);
        if let Some(k) = key {
            b = b.header("x-api-key", k);
        }
        let mut r = b
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .expect("request");
        r.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9))));
        r
    }

    async fn status(app: &Router, r: Request<Body>) -> StatusCode {
        app.clone().oneshot(r).await.expect("response").status()
    }

    #[tokio::test]
    async fn api_requires_auth() {
        let app = app();
        assert_eq!(
            status(&app, req("GET", "/api/health", None, "")).await,
            StatusCode::OK
        );
        for uri in ["/api/recordings", "/api/recordings/1", "/api/auth/me"] {
            assert_eq!(
                status(&app, req("GET", uri, None, "")).await,
                StatusCode::UNAUTHORIZED
            );
        }
        // A forged or tampered session cookie is no better than none.
        let mut r = req("GET", "/api/recordings", None, "");
        r.headers_mut().insert(
            header::COOKIE,
            "liberarr_session=99999999999.deadbeef"
                .parse()
                .expect("header"),
        );
        assert_eq!(status(&app, r).await, StatusCode::UNAUTHORIZED);
        // A real login cookie opens the API.
        let login = app
            .clone()
            .oneshot(req(
                "POST",
                "/api/auth/login",
                None,
                r#"{"username":"admin","password":"pw"}"#,
            ))
            .await
            .expect("response");
        assert_eq!(login.status(), StatusCode::NO_CONTENT);
        let set = login.headers()[header::SET_COOKIE]
            .to_str()
            .expect("cookie");
        assert!(set.contains("HttpOnly") && set.contains("SameSite=Lax"));
        let mut r = req("GET", "/api/recordings", None, "");
        r.headers_mut().insert(
            header::COOKIE,
            set.split(';')
                .next()
                .expect("pair")
                .parse()
                .expect("header"),
        );
        assert_eq!(status(&app, r).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn login_rate_limited_after_5_failures() {
        let app = app();
        let bad = r#"{"username":"admin","password":"nope"}"#;
        for _ in 0..5 {
            assert_eq!(
                status(&app, req("POST", "/api/auth/login", None, bad)).await,
                StatusCode::UNAUTHORIZED
            );
        }
        // Even the right password is refused once the budget is spent.
        let good = r#"{"username":"admin","password":"pw"}"#;
        assert_eq!(
            status(&app, req("POST", "/api/auth/login", None, good)).await,
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[tokio::test]
    async fn api_key_header_authorizes() {
        let app = app();
        assert_eq!(
            status(&app, req("GET", "/api/recordings", Some("key"), "")).await,
            StatusCode::OK
        );
        assert_eq!(
            status(&app, req("GET", "/api/recordings", Some("wrong"), "")).await,
            StatusCode::UNAUTHORIZED
        );
        // Unknown step is a 400 before anything is queued; a known one is accepted.
        let rerun = |step: &str| {
            req(
                "POST",
                "/api/recordings/1/rerun",
                Some("key"),
                &format!(r#"{{"step":"{step}"}}"#),
            )
        };
        assert_eq!(status(&app, rerun("bogus")).await, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_api_is_404_not_spa() {
        let res = app()
            .oneshot(req("GET", "/api/nope", Some("key"), ""))
            .await
            .expect("response");
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(res.headers()[header::CONTENT_TYPE], "application/json");
    }

    #[tokio::test]
    async fn spa_fallback_serves_index() {
        let res = app()
            .oneshot(req("GET", "/recordings/42", None, ""))
            .await
            .expect("response");
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .expect("body");
        assert!(String::from_utf8_lossy(&body).contains("<html"));
    }
}
