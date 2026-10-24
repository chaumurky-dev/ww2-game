//! Response hardening: `no-store` for the API, CSP and friends for the SPA.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use axum::extract::{Request, State};
use axum::http::{header, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use base64::Engine;
use sha2::{Digest, Sha256};

/// Maximum accepted request body.
pub const BODY_LIMIT: usize = 1024 * 1024;

/// Content-Security-Policy source list for the inline scripts found in `index.html`
/// (trunk emits an inline module that boots the wasm), as `'sha256-...'` hashes.
pub fn inline_script_hashes(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("<script") {
        rest = &rest[i + 7..];
        let Some(gt) = rest.find('>') else { break };
        let attrs = &rest[..gt];
        let after = &rest[gt + 1..];
        let Some(end) = after.find("</script>") else {
            break;
        };
        if !attrs.contains("src=") {
            let digest = Sha256::digest(&after.as_bytes()[..end]);
            out.push(format!(
                "'sha256-{}'",
                base64::engine::general_purpose::STANDARD.encode(digest)
            ));
        }
        rest = &after[end..];
    }
    out
}

pub fn build_csp(index_html: Option<&str>) -> String {
    let mut script = String::from("script-src 'self' 'wasm-unsafe-eval'");
    for h in index_html.map(inline_script_hashes).unwrap_or_default() {
        script.push(' ');
        script.push_str(&h);
    }
    [
        "default-src 'self'",
        &script,
        "style-src 'self' 'unsafe-inline' https://fonts.googleapis.com",
        "font-src 'self' https://fonts.gstatic.com data:",
        "img-src 'self' data:",
        "connect-src 'self' ws: wss:",
        "object-src 'none'",
        "base-uri 'self'",
        "form-action 'self'",
        "frame-ancestors 'none'",
    ]
    .join("; ")
}

/// Computes the CSP from the served `index.html`, re-reading it when the file changes.
pub struct Hardening {
    index: PathBuf,
    cache: Mutex<Option<(Option<SystemTime>, HeaderValue)>>,
}

impl Hardening {
    pub fn new(index: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            index,
            cache: Mutex::new(None),
        })
    }

    fn csp(&self) -> HeaderValue {
        let mtime = std::fs::metadata(&self.index)
            .and_then(|m| m.modified())
            .ok();
        let mut cache = self.cache.lock().unwrap();
        if let Some((m, v)) = cache.as_ref() {
            if *m == mtime {
                return v.clone();
            }
        }
        let html = std::fs::read_to_string(&self.index).ok();
        let v = HeaderValue::from_str(&build_csp(html.as_deref()))
            .unwrap_or_else(|_| HeaderValue::from_static("default-src 'self'"));
        *cache = Some((mtime, v.clone()));
        v
    }
}

pub async fn harden(State(h): State<Arc<Hardening>>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if path == "/api" || path.starts_with("/api/") {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    } else if path != "/ws" {
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
        headers.insert(header::CONTENT_SECURITY_POLICY, h.csp());
        headers.insert(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        );
        headers.insert(
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        );
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_only_inline_scripts() {
        let html = r#"<script src="/a.js"></script><script type="module">let x=1;</script>"#;
        let h = inline_script_hashes(html);
        assert_eq!(h.len(), 1);
        assert!(h[0].starts_with("'sha256-") && h[0].ends_with('\''));
    }

    #[test]
    fn csp_has_required_sources() {
        let c = build_csp(None);
        assert!(c.contains("script-src 'self'"));
        assert!(c.contains("https://fonts.googleapis.com"));
        assert!(c.contains("https://fonts.gstatic.com"));
        assert!(c.contains("img-src 'self' data:"));
        assert!(c.contains("connect-src 'self' ws: wss:"));
        assert!(c.contains("frame-ancestors 'none'"));
        assert!(!c.contains("unsafe-eval'") || c.contains("wasm-unsafe-eval"));
    }
}
