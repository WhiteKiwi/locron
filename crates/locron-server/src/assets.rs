//! Bundled static assets for the single-page viewer, served by one rust-embed-backed handler.
//!
//! `index.html` is the entry page: it hosts the one-time token paste and, once a session cookie
//! exists, the viewer shell. The entry page and every asset it references are public (GETs
//! outside `/api/` are exempt from token authentication) because the bundle must load before any
//! token exists — the paste form itself is served by `app.js` — and the bundle carries no data.
//! Every `/api/v1` route stays token-gated by the middleware chain.

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

    /// Script and stylesheet references the entry page must resolve.
    const ASSET_REFERENCES: &[(&str, &str)] = &[
        ("/router.js", "text/javascript"),
        ("/api.js", "text/javascript"),
        ("/components.js", "text/javascript"),
        ("/sse.js", "text/javascript"),
        ("/views/jobs.js", "text/javascript"),
        ("/views/runs.js", "text/javascript"),
        ("/views/diagnostics.js", "text/javascript"),
        ("/app.js", "text/javascript"),
        ("/app.css", "text/css"),
    ];

    #[test]
    fn every_referenced_asset_is_embedded() {
        let index = Assets::get("index.html").expect("index.html is embedded");
        let html = String::from_utf8_lossy(&index.data);
        for (reference, _) in ASSET_REFERENCES {
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
    fn referenced_assets_are_served_with_correct_content_types() {
        for (reference, expected) in ASSET_REFERENCES {
            let content = Assets::get(reference.trim_start_matches('/'))
                .expect("reference must resolve to an embedded asset");
            assert_eq!(
                content.metadata.mimetype(),
                *expected,
                "content type for {reference}"
            );
        }
    }

    #[test]
    fn every_embedded_view_script_registers_routes() {
        // The view scripts are plain IIFEs that register routes at load time;
        // this smoke check ensures the bundles are present and non-empty.
        for path in ["views/jobs.js", "views/runs.js", "views/diagnostics.js"] {
            let content = Assets::get(path).expect("view script is embedded");
            let script = String::from_utf8_lossy(&content.data);
            assert!(
                script.contains("Router.register"),
                "{path} must register routes"
            );
        }
    }
}
