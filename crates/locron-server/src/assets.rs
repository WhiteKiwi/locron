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
        assert!(html.contains("class=\"skip-link\""));
        assert!(html.contains("<main id=\"main-content\""));
        assert!(html.contains("<nav aria-label=\"Dashboard\""));
        assert!(!html.contains("http://") && !html.contains("https://"));
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

    #[test]
    fn viewer_bootstrap_and_http_form_follow_the_wire_contract() {
        let app = Assets::get("app.js").expect("app script");
        let app = String::from_utf8_lossy(&app.data);
        assert!(app.contains("data.authenticated === true"));
        assert!(!app.contains("Api.hasSession"));

        let api = Assets::get("api.js").expect("api script");
        let api = String::from_utf8_lossy(&api.data);
        assert!(
            !api.contains("locron_session"),
            "HttpOnly session is never read by JS"
        );

        let jobs = Assets::get("views/jobs.js").expect("jobs view");
        let jobs = String::from_utf8_lossy(&jobs.data);
        assert!(jobs.contains("new TextEncoder().encode(bodyInput.value)"));
        assert!(!jobs.contains("<option>OPTIONS</option>"));
        assert!(jobs.contains(r#"const action = job.enabled ? "disable" : "enable";"#));
        assert!(jobs.contains(
            ".map((entry) => entry.trim())\n        .filter(Boolean)\n        .map(Number)"
        ));
        assert!(
            !jobs.contains("Number(entry.trim())"),
            "empty success-status entries must be removed before numeric conversion"
        );

        let stream = Assets::get("sse.js").expect("SSE client");
        let stream = String::from_utf8_lossy(&stream.data);
        assert!(stream.contains("`${data.attempt_number}:${data.seq}`"));
        assert!(stream.contains("seenOutput.has(key)"));
    }

    #[test]
    fn viewer_accessibility_and_brand_semantics_stay_scoped() {
        let html = Assets::get("index.html").expect("entry page");
        let html = String::from_utf8_lossy(&html.data);
        assert!(html.contains(r#"<div id="view"></div>"#));
        assert!(!html.contains(r#"id="view" aria-live"#));

        let css = Assets::get("app.css").expect("stylesheet");
        let css = String::from_utf8_lossy(&css.data);
        assert!(!css.contains(".page-head h1::before"));
        assert!(!css.contains(r#"content: "Local operations""#));
        assert!(css.contains(r#"input[type="checkbox"], input[type="radio"]"#));
        assert!(css.contains("width: 1.125rem; height: 1.125rem; min-height: 1.125rem"));
        assert_eq!(
            css.matches("color: var(--color-caution);").count(),
            2,
            "only warning and notice text use the caution semantic"
        );
    }

    #[test]
    fn documented_palette_matches_the_css_token_layer() {
        let guide = include_str!("../../../DESIGN.md");
        let css = Assets::get("app.css").expect("stylesheet");
        let css = String::from_utf8_lossy(&css.data);
        for (token, value) in [
            ("--color-canvas", "#F6F0E3"),
            ("--color-surface", "#FFFCF6"),
            ("--color-raised", "#FFFFFF"),
            ("--color-ink", "#24231F"),
            ("--color-graphite", "#5F5B52"),
            ("--color-border", "#D8D0C1"),
            ("--color-accent", "#F5C842"),
            ("--color-link", "#355B88"),
            ("--color-success", "#246B45"),
            ("--color-success-soft", "#E7F2EA"),
            ("--color-danger", "#A83B35"),
            ("--color-danger-soft", "#F8E8E5"),
            ("--color-running", "#285D8F"),
            ("--color-running-soft", "#E7EFF7"),
            ("--color-caution", "#875B12"),
            ("--color-caution-soft", "#F6EEDC"),
            ("--color-unknown", "#665F54"),
            ("--color-unknown-soft", "#EFEBE3"),
            ("--color-console", "#171713"),
            ("--color-console-ink", "#F4F0E7"),
        ] {
            assert!(
                guide.contains(&format!("`{token}` | `{value}`")),
                "brand guide token {token} must carry {value}"
            );
            assert!(
                css.contains(&format!("{token}: {value};")),
                "CSS token {token} must carry {value}"
            );
        }
    }
}
