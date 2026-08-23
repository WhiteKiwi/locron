//! Bundled static assets for the single-page viewer, served by one rust-embed-backed handler.
//!
//! `index.html` is the entry page: the only unauthenticated response. It hosts the one-time token
//! paste and, once a session cookie exists, the viewer shell. All other assets require a valid
//! session or bearer token, enforced by the middleware chain.

use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::Response;
use rust_embed::RustEmbed;

/// The bundled viewer assets (the `assets/` directory next to this crate's manifest).
#[derive(RustEmbed)]
#[folder = "assets/"]
pub struct Assets;

/// Serves the entry page at `GET /`.
pub async fn entry() -> Response {
    serve_asset("index.html")
}

/// Serves any other embedded asset at `GET /{*path}`; unknown paths are 404.
pub async fn assets(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return serve_asset("index.html");
    }
    serve_asset(path)
}

fn serve_asset(path: &str) -> Response {
    match Assets::get(path) {
        Some(content) => {
            let body = Body::from(content.data.into_owned());
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content.metadata.mimetype())
                .header(header::CACHE_CONTROL, "no-cache")
                .body(body)
                .expect("static asset responses are valid")
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from("not found"))
            .expect("static 404 responses are valid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_referenced_asset_is_embedded() {
        let index = Assets::get("index.html").expect("index.html is embedded");
        let html = String::from_utf8_lossy(&index.data);
        for reference in ["/app.js", "/app.css"] {
            assert!(
                html.contains(reference),
                "entry page must reference {reference}"
            );
            assert!(
                Assets::get(reference.trim_start_matches('/')).is_some(),
                "{reference} must be embedded"
            );
        }
    }

    #[test]
    fn content_types_are_guessed() {
        assert_eq!(
            Assets::get("index.html").unwrap().metadata.mimetype(),
            "text/html"
        );
    }
}
