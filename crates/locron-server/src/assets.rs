//! Deterministic Vite output embedded into the local dashboard server.
//!
//! The browser source lives in `frontend/`; Cargo never invokes Node. The checked-in
//! `frontend/dist/` tree is the complete public bundle and contains no operator data.

use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::Response;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
/// The complete checked-in browser bundle.
pub struct Assets;

/// Serve the dashboard entry document.
pub async fn entry() -> Response {
    serve_asset("index.html")
}

/// Serve a referenced embedded asset or a local 404.
pub async fn assets(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    serve_asset(if path.is_empty() { "index.html" } else { path })
}

fn serve_asset(path: &str) -> Response {
    match Assets::get(path) {
        Some(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content.metadata.mimetype())
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(content.data.into_owned()))
            .expect("static asset responses are valid"),
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
    use sha2::{Digest, Sha256};
    use std::path::Path;

    fn text(path: &str) -> String {
        String::from_utf8_lossy(
            &Assets::get(path)
                .unwrap_or_else(|| panic!("{path} embedded"))
                .data,
        )
        .into_owned()
    }

    fn referenced_assets(index: &str) -> Vec<String> {
        index
            .split(['\"', '\''])
            .filter(|value| {
                value.starts_with("/assets/")
                    || *value == "/favicon.svg"
                    || value.starts_with("/fonts/")
            })
            .map(|value| value.trim_start_matches('/').to_owned())
            .collect()
    }

    fn extension_is(path: &str, expected: &str) -> bool {
        Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
    }

    fn relative_luminance(hex: &str) -> f64 {
        let channel = |start| {
            let value =
                f64::from(u8::from_str_radix(&hex[start..start + 2], 16).expect("hex color"))
                    / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
    }

    fn contrast_ratio(first: &str, second: &str) -> f64 {
        let first = relative_luminance(first);
        let second = relative_luminance(second);
        (first.max(second) + 0.05) / (first.min(second) + 0.05)
    }

    #[test]
    fn built_entry_references_only_embedded_local_assets() {
        let index = text("index.html");
        let references = referenced_assets(&index);
        assert!(references.iter().any(|path| extension_is(path, "js")));
        assert!(references.iter().any(|path| extension_is(path, "css")));
        assert!(references.contains(&"favicon.svg".to_owned()));
        assert_eq!(index.matches("rel=\"preload\"").count(), 1);
        for path in references {
            assert!(Assets::get(&path).is_some(), "{path} must resolve");
        }
        for path in Assets::iter() {
            let asset = text(path.as_ref());
            if extension_is(path.as_ref(), "html") || extension_is(path.as_ref(), "css") {
                assert!(
                    !asset.contains("http://") && !asset.contains("https://"),
                    "{path} must not fetch remote assets"
                );
            }
            if extension_is(path.as_ref(), "svg") {
                assert!(
                    !asset.contains("<script") && !asset.contains("href=\"http"),
                    "{path} must be inert and local"
                );
            }
            if extension_is(path.as_ref(), "js") {
                assert!(
                    !asset.contains("fetch(\"http")
                        && !asset.contains("src=\"http")
                        && !asset.contains("href=\"http"),
                    "{path} must not make a remote runtime request"
                );
            }
            assert!(
                !extension_is(path.as_ref(), "map"),
                "source maps are not shipped"
            );
        }
    }

    #[test]
    fn assets_have_browser_content_types() {
        let index = text("index.html");
        for path in referenced_assets(&index) {
            let asset = Assets::get(&path).expect("asset");
            let mime = asset.metadata.mimetype();
            if extension_is(&path, "js") {
                assert!(mime.contains("javascript"));
            }
            if extension_is(&path, "css") {
                assert_eq!(mime, "text/css");
            }
            if extension_is(&path, "woff2") {
                assert_eq!(mime, "font/woff2");
            }
            if extension_is(&path, "svg") {
                assert_eq!(mime, "image/svg+xml");
            }
        }
    }

    #[test]
    fn prepaint_theme_and_browser_identity_are_ordered() {
        let index = text("index.html");
        let bootstrap = index.find("locron.theme").expect("theme bootstrap");
        let stylesheet = index.find("stylesheet").expect("stylesheet");
        let body = index.find("<body").expect("body");
        assert!(bootstrap < stylesheet && bootstrap < body);
        for value in [
            "system",
            "light",
            "dark",
            "matchMedia",
            "dataset.theme",
            "colorScheme",
            "theme-color",
        ] {
            assert!(index.contains(value), "bootstrap includes {value}");
        }
        assert!(index.contains("favicon.svg"));
        assert!(include_str!("../frontend/src/App.tsx").contains("titleForRoute"));
        for title in [
            "Jobs · Locron",
            "New job · Locron",
            "Run history · Locron",
            "Diagnostics · Locron",
            "Settings · Locron",
            "Locron dashboard",
        ] {
            assert!(include_str!("../frontend/src/App.tsx").contains(title));
        }
    }

    #[test]
    fn documented_theme_tokens_match_built_css() {
        let guide = include_str!("../../../DESIGN.md");
        let source_css = include_str!("../assets/app.css");
        let css_path = Assets::iter()
            .find(|path| extension_is(path.as_ref(), "css"))
            .expect("built CSS");
        let css = text(css_path.as_ref());
        for (token, value) in [
            ("--color-canvas", "#F7F5EF"),
            ("--color-surface", "#FCFBF7"),
            ("--color-raised", "#FFFFFF"),
            ("--color-hover", "#F4F0E6"),
            ("--color-pressed", "#EBE5D7"),
            ("--color-selected", "#FFF0C2"),
            ("--color-border", "#D9D5CA"),
            ("--color-border-control", "#8D887E"),
            ("--color-text", "#211F1A"),
            ("--color-muted", "#6A655B"),
            ("--color-disabled-text", "#817C72"),
            ("--color-accent", "#E3A91D"),
            ("--color-accent-text", "#7A4A00"),
            ("--color-accent-soft", "#FFF0C2"),
            ("--color-on-accent", "#241A00"),
            ("--color-focus", "#B87500"),
            ("--color-primary", "#211F1A"),
            ("--color-on-primary", "#FFFFFF"),
            ("--color-success", "#176B4C"),
            ("--color-warning", "#795000"),
            ("--color-danger", "#A73531"),
            ("--color-info", "#245E8C"),
            ("--color-console", "#171713"),
            ("--color-canvas", "#151512"),
            ("--color-surface", "#1C1C18"),
            ("--color-raised", "#24231E"),
            ("--color-hover", "#25241F"),
            ("--color-pressed", "#2D2B24"),
            ("--color-selected", "#3A2C0D"),
            ("--color-border", "#3A3931"),
            ("--color-border-control", "#747164"),
            ("--color-text", "#F3F0E8"),
            ("--color-muted", "#AAA69B"),
            ("--color-disabled-text", "#858176"),
            ("--color-accent", "#E4AD2B"),
            ("--color-accent-text", "#F0BD4C"),
            ("--color-accent-soft", "#3A2C0D"),
            ("--color-on-accent", "#201800"),
            ("--color-focus", "#E4AD2B"),
            ("--color-primary", "#F3F0E8"),
            ("--color-on-primary", "#151512"),
            ("--color-success", "#70D4A7"),
            ("--color-warning", "#F0BD4C"),
            ("--color-danger", "#F07872"),
            ("--color-info", "#83B9EB"),
            ("--color-console", "#0F0F0D"),
            ("--color-console-text", "#F3F0E8"),
        ] {
            assert!(
                guide.contains(&format!("`{token}`")) && guide.contains(&format!("`{value}`")),
                "guide has {token} {value}"
            );
            assert!(
                source_css.contains(&format!("{token}:{value}"))
                    || (token == "--color-accent-soft"
                        && source_css.contains("--color-accent-soft:var(--color-selected)")),
                "source CSS has {token} {value}"
            );
            assert!(
                css.contains(&format!("{token}:")),
                "built CSS retains {token}"
            );
        }
        for contract in [
            "grid-template-columns:176px minmax(0,720px)",
            "width:224px",
            "width:64px",
            "prefers-reduced-transparency:reduce",
            "prefers-contrast:more",
            "forced-colors:active",
            "prefers-reduced-motion:reduce",
            ":focus-visible",
        ] {
            assert!(css.contains(contract), "CSS contains {contract}");
        }
        for responsive in [
            "max-width:767px",
            ".mobile-data{display:block}",
            "data-material=\"solid\"",
        ] {
            assert!(
                source_css.contains(responsive),
                "source CSS contains {responsive}"
            );
        }
        for rejected in [
            "linear-gradient",
            "radial-gradient",
            "filter:drop-shadow",
            "text-shadow",
        ] {
            assert!(
                !source_css.contains(rejected),
                "flat CSS rejects {rejected}"
            );
        }
        for material in [
            "rgb(252 251 247 / .86)",
            "rgb(28 28 24 / .82)",
            "rgb(255 255 255 / .92)",
            "rgb(36 35 30 / .90)",
            "blur(14px) saturate(108%)",
            "blur(16px) saturate(110%)",
        ] {
            assert!(
                source_css.contains(material),
                "material includes {material}"
            );
        }
        assert!(!source_css.contains("button:disabled{opacity"));
        assert!(!source_css.contains("Pretendard"));
        assert!(!source_css.contains(".clickable-row:hover{box-shadow"));
    }

    #[test]
    fn documented_contrast_targets_are_met() {
        for (foreground, background) in [
            ("#211F1A", "#F7F5EF"),
            ("#6A655B", "#F7F5EF"),
            ("#241A00", "#E3A91D"),
            ("#FFFFFF", "#211F1A"),
            ("#176B4C", "#E7F5EE"),
            ("#795000", "#FFF3CC"),
            ("#A73531", "#FCECEA"),
            ("#245E8C", "#EAF3FC"),
            ("#F3F0E8", "#151512"),
            ("#AAA69B", "#151512"),
            ("#201800", "#E4AD2B"),
            ("#151512", "#F3F0E8"),
            ("#70D4A7", "#193329"),
            ("#F0BD4C", "#382B0D"),
            ("#F07872", "#3D211F"),
            ("#83B9EB", "#1D2E3D"),
        ] {
            assert!(
                contrast_ratio(foreground, background) >= 4.5,
                "{foreground} on {background} must meet 4.5:1"
            );
        }
        for (boundary, surface) in [
            ("#8D887E", "#FCFBF7"),
            ("#B87500", "#FCFBF7"),
            ("#747164", "#1C1C18"),
            ("#E4AD2B", "#1C1C18"),
        ] {
            assert!(
                contrast_ratio(boundary, surface) >= 3.0,
                "{boundary} against {surface} must meet 3:1"
            );
        }
    }

    #[test]
    fn finish_quality_source_contracts_are_local_and_semantic() {
        let viewer = include_str!("../frontend/src/json.tsx");
        for contract in [
            "lexJson",
            "JSON.parse(source)",
            "navigator.clipboard.writeText(source)",
            "locron.json.wrap",
            "65_536",
            "jsonPreview(source)",
            "Invalid JSON",
            "<pre className=",
            "<code>",
        ] {
            assert!(viewer.contains(contract), "JSON viewer contains {contract}");
        }
        for dependency in ["monaco", "codemirror", "shiki"] {
            assert!(
                !include_str!("../frontend/package.json")
                    .to_lowercase()
                    .contains(dependency)
            );
        }
        let jobs = include_str!("../frontend/src/routes/Jobs.tsx");
        let runs = include_str!("../frontend/src/routes/Runs.tsx");
        for source in [jobs, runs] {
            assert!(source.contains("navigateRow"));
            assert!(source.contains("data-row-link"));
            assert!(!source.contains("role=\"link\"") && !source.contains("tabIndex={0}"));
        }
        assert!(!jobs.contains("json-block") && !runs.contains("json-block"));
        assert!(jobs.contains("JsonViewer") && runs.contains("JsonViewer"));
        let shell = include_str!("../frontend/src/AppShell.tsx");
        assert!(shell.contains("LabelledNavItems") && shell.contains("CompactNavItems"));
        assert!(
            !shell.contains("Tooltip key={key} label={label}><a href={`#/${key}`} aria-current")
        );
    }

    #[test]
    fn responsive_width_and_select_accessibility_contracts_are_explicit() {
        let css = include_str!("../assets/app.css");
        for contract in [
            ".app-shell,.shell-workbench,#main-content,.desktop-data,.table-scroll{min-width:0}",
            ".table-scroll{max-width:100%;overscroll-behavior-inline:contain}",
            ".search-input>input[type=\"search\"]{width:auto;min-width:0;flex:1 1 auto}",
            "@media(max-width:1023px) and (min-width:768px){.route-header{margin:-24px -24px 24px}}",
            ".form-section-nav{position:static;display:flex;overflow-x:auto",
            ".form-section-nav::-webkit-scrollbar{display:none}",
            "scrollbar-width:none",
            ".form-section-nav button[aria-current=\"step\"]",
            ".select-shell>select[aria-hidden=\"true\"]{display:none!important}",
        ] {
            assert!(css.contains(contract), "responsive CSS contains {contract}");
        }
        assert!(!css.contains("body{overflow-x:hidden"));
        let form = include_str!("../frontend/src/routes/JobForm.tsx");
        for section in [
            "Identity",
            "Schedule",
            "Target",
            "Environment",
            "Policy",
            "Review",
        ] {
            assert!(form.contains(section), "form navigation retains {section}");
        }
        assert!(form.contains("aria-current={activeSection === section ? \"step\" : undefined}"));
        let select = include_str!("../frontend/src/ui.tsx");
        assert!(select.contains("className=\"select-shell\""));
        assert!(select.contains("<SelectPrimitive.Root value={value}"));
    }

    #[test]
    fn stable_empty_data_and_form_spacing_contracts_are_explicit() {
        let css = include_str!("../assets/app.css");
        for contract in [
            ".field{gap:0;margin-bottom:20px}",
            ".field>label+.field-control{margin-top:8px}",
            ".field-help,.toolbar .field-help,.field .error{margin:4px 0 0}",
            ".theme-options{display:flex;flex-wrap:wrap;gap:8px;margin-top:8px}",
            ".theme-group-help{max-width:56ch;margin:8px 0 0",
            ".form-section{padding:0 0 40px;margin:0 0 40px}",
            ".empty-table-row td,.empty-table-row td:last-child{width:auto;height:160px;padding:24px;text-align:center}",
            ".data-empty{display:flex;flex-direction:column;align-items:center;justify-content:center;min-height:112px;max-width:480px",
            ".empty-object-list .data-empty{min-height:96px;padding:24px 16px}",
        ] {
            assert!(css.contains(contract), "spacing CSS contains {contract}");
        }
        let components = include_str!("../frontend/src/components.tsx");
        assert!(components.contains("className=\"field-control\""));
        assert!(
            components.contains("<fieldset className=\"theme-control\" aria-describedby={help}>")
        );
        assert!(components.contains("className=\"theme-group-help\""));
        let settings = include_str!("../frontend/src/routes/Settings.tsx");
        assert_eq!(settings.matches("Browser-local only").count(), 0);

        let ui = include_str!("../frontend/src/ui.tsx");
        assert!(ui.contains("<td colSpan={columns}>"));
        assert!(ui.contains("className=\"object-list empty-object-list\""));
        assert!(!ui.contains("className=\"data-empty\" role=\"status\""));
        let jobs = include_str!("../frontend/src/routes/Jobs.tsx");
        for contract in [
            "<EmptyTableRow columns={5}",
            "aria-describedby=\"jobs-results-status\"",
            "setState(\"all\")",
            "search.current?.focus()",
        ] {
            assert!(jobs.contains(contract), "Jobs contains {contract}");
        }
        let runs = include_str!("../frontend/src/routes/Runs.tsx");
        for contract in [
            "<EmptyTableRow columns={6}",
            "aria-describedby=\"runs-results-status\"",
            "total > 0 && <div className=\"pager\"",
            "input.current?.focus()",
        ] {
            assert!(runs.contains(contract), "Runs contains {contract}");
        }
    }

    #[test]
    fn typed_primitives_are_exactly_pinned_and_portalled() {
        let package = include_str!("../frontend/package.json");
        for pin in [
            "\"@radix-ui/react-select\": \"2.3.7\"",
            "\"@radix-ui/react-dropdown-menu\": \"2.1.24\"",
            "\"@radix-ui/react-dialog\": \"1.1.23\"",
            "\"@radix-ui/react-tooltip\": \"1.2.16\"",
            "\"lucide-react\": \"1.34.0\"",
        ] {
            assert!(package.contains(pin), "package contains {pin}");
        }
        assert!(text("index.html").contains("id=\"portal-root\""));
        let primitives = include_str!("../frontend/src/ui.tsx");
        for contract in [
            "LocronSelect",
            "ActionMenu",
            "Dialog",
            "Tooltip",
            "portal-root",
            "StatusBadge",
        ] {
            assert!(primitives.contains(contract));
        }
        let shell = include_str!("../frontend/src/AppShell.tsx");
        for contract in [
            "side-rail",
            "mobile-topbar",
            "mobile-nav",
            "aria-current",
            "Daemon",
        ] {
            assert!(shell.contains(contract));
        }
    }

    #[test]
    fn official_fonts_license_and_provenance_are_embedded() {
        for (path, expected) in [
            (
                "fonts/GeistSans-Variable.woff2",
                "a369fcf5628ea2aa4e1b9e2ec6a5b3624e365bda588e1f0f2f12b564f728fbb8",
            ),
            (
                "fonts/GeistMono-Variable.woff2",
                "fba8f577f38a2bbcbe818efa6348dd58f36303a10b8737c42fefad275be563ab",
            ),
        ] {
            let asset = Assets::get(path).expect("font");
            assert_eq!(asset.metadata.mimetype(), "font/woff2");
            assert_eq!(
                format!("{:x}", Sha256::digest(asset.data.as_ref())),
                expected
            );
        }
        let license = text("fonts/OFL.txt");
        assert!(license.contains("SIL OPEN FONT LICENSE Version 1.1"));
        assert_eq!(
            format!("{:x}", Sha256::digest(license.as_bytes())),
            "2b2da563e79400b61818402ca9f26a73d52468268b7fc715e92143c1e799737e"
        );
        assert!(
            include_str!("../../../DESIGN.md")
                .contains("official `vercel/geist-font` tag `v1.7.2`")
        );
    }

    #[test]
    fn typed_source_keeps_auth_search_sse_and_input_contracts() {
        let app = include_str!("../frontend/src/App.tsx");
        assert!(app.contains("data.authenticated === true"));
        assert!(!include_str!("../frontend/src/api.ts").contains("locron_session"));
        let jobs = include_str!("../frontend/src/routes/JobForm.tsx");
        for contract in [
            "TextEncoder",
            "parseSuccessStatuses",
            "HTTP headers",
            "Environment file",
            "PATH override",
            "Preview next 5 occurrences",
            "ValidationSummary",
        ] {
            assert!(jobs.contains(contract));
        }
        assert!(!jobs.contains("OPTIONS"));
        let runs = include_str!("../frontend/src/routes/Runs.tsx");
        for contract in [
            "250",
            "AbortController",
            "generation.current",
            "outputEventKey",
            "attempt_number",
            "EventSource",
            "removeEventListener",
        ] {
            assert!(runs.contains(contract));
        }
        let diagnostics = include_str!("../frontend/src/routes/Diagnostics.tsx");
        assert!(!diagnostics.contains("api.put") && !diagnostics.contains("api.delete"));
        let settings = include_str!("../frontend/src/routes/Settings.tsx");
        for contract in [
            "ThemeControl",
            "run_retention_age_us",
            "ByteSizeInput",
            "Review durable change",
            "execution search path",
        ] {
            assert!(settings.contains(contract));
        }
    }

    #[test]
    fn app_is_split_into_route_components() {
        let app = include_str!("../frontend/src/App.tsx");
        assert!(app.lines().count() < 170, "shell stays small");
        for module in ["Diagnostics", "Jobs", "JobForm", "Runs", "Settings"] {
            assert!(app.contains(&format!("./routes/{module}")));
        }
    }
}
