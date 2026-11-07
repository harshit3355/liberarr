//! The embedded frontend and its SPA fallback.
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

/// The built frontend (`web/dist`), embedded at compile time.
#[derive(RustEmbed)]
#[folder = "../../web/dist"]
pub struct Assets;

/// An embedded file, or `index.html` for client-side routes. A missing file with an extension
/// (a stale `/assets/x.js`) is a real 404, not the SPA shell.
pub async fn spa(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let file = match Assets::get(path) {
        Some(f) if !path.is_empty() => f,
        _ if path.rsplit('/').next().is_some_and(|s| s.contains('.')) => {
            return StatusCode::NOT_FOUND.into_response();
        }
        _ => match Assets::get("index.html") {
            Some(f) => f,
            None => return StatusCode::NOT_FOUND.into_response(),
        },
    };
    (
        [(header::CONTENT_TYPE, file.metadata.mimetype().to_owned())],
        file.data.into_owned(),
    )
        .into_response()
}
