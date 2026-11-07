//! HTTP API and the embedded web UI.

pub mod assets {
    use rust_embed::RustEmbed;

    /// The built frontend (`web/dist`), embedded at compile time.
    #[derive(RustEmbed)]
    #[folder = "../../web/dist"]
    pub struct Assets;
}
