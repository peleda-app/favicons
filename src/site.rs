use std::sync::LazyLock;

use axum::body::Body;
use axum::response::Response;
use base64::Engine;
use http::header::{CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, IF_NONE_MATCH};
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};

const INDEX: &str = include_str!("../site/index.html");
const FAVICON: &[u8] = include_bytes!("../site/favicon.svg");

pub struct Page {
    body: &'static [u8],
    content_type: &'static str,
    cache_control: &'static str,
    etag: HeaderValue,
    csp: HeaderValue,
}

pub static INDEX_PAGE: LazyLock<Page> = LazyLock::new(|| Page {
    body: INDEX.as_bytes(),
    content_type: "text/html; charset=utf-8",
    cache_control: "public, max-age=300, stale-while-revalidate=86400",
    etag: etag(INDEX.as_bytes()),
    csp: index_csp(INDEX),
});

pub static FAVICON_PAGE: LazyLock<Page> = LazyLock::new(|| Page {
    body: FAVICON,
    content_type: "image/svg+xml",
    cache_control: "public, max-age=86400",
    etag: etag(FAVICON),
    csp: HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'"),
});

fn etag(body: &[u8]) -> HeaderValue {
    let hash = blake3::hash(body).to_hex();
    HeaderValue::from_str(&format!("\"{}\"", &hash[..20])).expect("hex etag")
}

fn index_csp(html: &str) -> HeaderValue {
    let script = inline_script(html).expect("index.html has one inline <script>");
    let digest = ring::digest::digest(&ring::digest::SHA256, script.as_bytes());
    let hash = base64::engine::general_purpose::STANDARD.encode(digest.as_ref());
    HeaderValue::from_str(&format!(
        "default-src 'none'; script-src 'sha256-{hash}'; style-src 'unsafe-inline'; \
         img-src 'self' blob:; connect-src 'self'; base-uri 'none'; form-action 'none'; \
         frame-ancestors 'none'"
    ))
    .expect("ascii csp")
}

fn inline_script(html: &str) -> Option<&str> {
    let start = html.find("<script>")? + "<script>".len();
    let end = start + html[start..].find("</script>")?;
    Some(&html[start..end])
}

pub fn serve(page: &Page, headers: &HeaderMap) -> Response {
    let fresh = headers
        .get(IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .zip(page.etag.to_str().ok())
        .is_some_and(|(inm, e)| crate::http::etag_matches(inm, e));
    let mut r = if fresh {
        let mut r = Response::new(Body::empty());
        *r.status_mut() = StatusCode::NOT_MODIFIED;
        r
    } else {
        Response::new(Body::from(page.body))
    };
    let h = r.headers_mut();
    h.insert(CONTENT_TYPE, HeaderValue::from_static(page.content_type));
    h.insert(CACHE_CONTROL, HeaderValue::from_static(page.cache_control));
    h.insert(ETAG, page.etag.clone());
    h.insert(CONTENT_SECURITY_POLICY, page.csp.clone());
    h.insert(
        HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("all"),
    );
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_hash_covers_the_inline_script() {
        let script = inline_script(INDEX).unwrap();
        assert!(script.contains("/v1/icon/"));
        let csp = INDEX_PAGE.csp.to_str().unwrap();
        assert!(csp.contains("script-src 'sha256-"));
        assert!(!csp.contains("unsafe-eval"));
        assert_eq!(INDEX.matches("<script").count(), 1);
    }

    #[test]
    fn page_loads_nothing_from_other_origins() {
        for needle in ["googleapis", "gstatic", "src=\"http", "href=\"//"] {
            assert!(!INDEX.contains(needle), "{needle}");
        }
    }
}
