use std::sync::{Arc, LazyLock};

use axum::body::Body;
use axum::response::Response;
use base64::Engine;
use bytes::Bytes;
use http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, HOST, IF_NONE_MATCH,
};
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};

use crate::config::Config;

const ROBOTS: &str = include_str!("../site/robots.txt");
const LLMS: &str = include_str!("../site/llms.txt");

#[cfg(feature = "site")]
mod page {
    pub const INDEX: &str = include_str!("../site/index.html");
    pub const SITEMAP: &str = include_str!("../site/sitemap.xml");
    pub const FAVICON: &[u8] = include_bytes!("../site/favicon.svg");
    pub const APPLE_TOUCH_ICON: &[u8] = include_bytes!("../site/apple-touch-icon.png");
    pub const OG_IMAGE: &[u8] = include_bytes!("../site/og.png");
    pub const FONT_SANS: &[u8] = include_bytes!("../site/fonts/inter.woff2");
    pub const FONT_MONO: &[u8] = include_bytes!("../site/fonts/jetbrains-mono.woff2");
}

#[cfg(not(feature = "site"))]
mod page {
    pub const INDEX: &str = "";
    pub const SITEMAP: &str = "";
    pub const FAVICON: &[u8] = &[];
    pub const APPLE_TOUCH_ICON: &[u8] = &[];
    pub const OG_IMAGE: &[u8] = &[];
    pub const FONT_SANS: &[u8] = &[];
    pub const FONT_MONO: &[u8] = &[];
}

pub const ENABLED: bool = cfg!(feature = "site");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    Index,
    Robots,
    Sitemap,
    Llms,
    OpenApi,
    Favicon,
    AppleTouchIcon,
    OgImage,
    FontSans,
    FontMono,
}

impl Asset {
    const TEMPLATED: [Asset; 5] = [
        Asset::Index,
        Asset::Robots,
        Asset::Sitemap,
        Asset::Llms,
        Asset::OpenApi,
    ];

    fn template(self) -> Option<&'static str> {
        match self {
            Asset::Index => Some(page::INDEX),
            Asset::Robots => Some(ROBOTS),
            Asset::Sitemap => Some(page::SITEMAP),
            Asset::Llms => Some(LLMS),
            _ => None,
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Asset::Index => "text/html; charset=utf-8",
            Asset::Robots | Asset::Llms => "text/plain; charset=utf-8",
            Asset::Sitemap => "application/xml; charset=utf-8",
            Asset::OpenApi => "application/json",
            Asset::Favicon => "image/svg+xml",
            Asset::AppleTouchIcon | Asset::OgImage => "image/png",
            Asset::FontSans | Asset::FontMono => "font/woff2",
        }
    }

    fn cache_control(self) -> &'static str {
        match self {
            Asset::Index => "public, max-age=300, stale-while-revalidate=86400",
            Asset::Robots | Asset::Sitemap | Asset::Llms | Asset::OpenApi => {
                "public, max-age=3600, stale-while-revalidate=86400"
            }
            Asset::Favicon | Asset::AppleTouchIcon | Asset::OgImage => {
                "public, max-age=86400, stale-while-revalidate=604800"
            }
            Asset::FontSans | Asset::FontMono => "public, max-age=31536000, immutable",
        }
    }

    fn indexable(self) -> bool {
        matches!(
            self,
            Asset::Index | Asset::Favicon | Asset::AppleTouchIcon | Asset::OgImage
        )
    }
}

pub struct Page {
    body: Bytes,
    asset: Asset,
    etag: HeaderValue,
    csp: Option<HeaderValue>,
}

impl Page {
    fn new(asset: Asset, body: Bytes) -> Self {
        let csp = match asset {
            Asset::Index => Some(index_csp(std::str::from_utf8(&body).unwrap_or_default())),
            Asset::Favicon => Some(HeaderValue::from_static(
                "default-src 'none'; style-src 'unsafe-inline'",
            )),
            _ => None,
        };
        Self {
            etag: etag(&body),
            body,
            asset,
            csp,
        }
    }

    fn rendered(asset: Asset, origin: &str, catalogs: &[String]) -> Self {
        if asset == Asset::OpenApi {
            let spec = crate::openapi::Spec {
                origin,
                catalogs,
                site: ENABLED,
            };
            return Page::new(asset, Bytes::from(spec.render()));
        }
        let template = asset.template().unwrap_or_default();
        Page::new(asset, Bytes::from(render(template, origin)))
    }
}

fn binary(asset: Asset) -> Option<&'static Page> {
    static FAVICON: LazyLock<Page> =
        LazyLock::new(|| Page::new(Asset::Favicon, Bytes::from_static(page::FAVICON)));
    static APPLE: LazyLock<Page> = LazyLock::new(|| {
        Page::new(
            Asset::AppleTouchIcon,
            Bytes::from_static(page::APPLE_TOUCH_ICON),
        )
    });
    static OG: LazyLock<Page> =
        LazyLock::new(|| Page::new(Asset::OgImage, Bytes::from_static(page::OG_IMAGE)));
    static SANS: LazyLock<Page> =
        LazyLock::new(|| Page::new(Asset::FontSans, Bytes::from_static(page::FONT_SANS)));
    static MONO: LazyLock<Page> =
        LazyLock::new(|| Page::new(Asset::FontMono, Bytes::from_static(page::FONT_MONO)));
    match asset {
        Asset::Favicon => Some(&FAVICON),
        Asset::AppleTouchIcon => Some(&APPLE),
        Asset::OgImage => Some(&OG),
        Asset::FontSans => Some(&SANS),
        Asset::FontMono => Some(&MONO),
        _ => None,
    }
}

pub struct Site {
    fixed: Option<Vec<Arc<Page>>>,
    catalogs: Vec<String>,
}

impl Site {
    pub fn new(cfg: &Config) -> Self {
        let catalogs: Vec<String> = cfg.catalogs.iter().map(|c| c.id.clone()).collect();
        let fixed = public_origin(cfg).map(|origin| {
            Asset::TEMPLATED
                .iter()
                .map(|a| Arc::new(Page::rendered(*a, &origin, &catalogs)))
                .collect()
        });
        Self { fixed, catalogs }
    }

    pub fn serve(&self, asset: Asset, headers: &HeaderMap, uri: &Uri) -> Response {
        if let Some(page) = binary(asset) {
            return respond(page, headers);
        }
        if let Some(page) = self
            .fixed
            .as_ref()
            .and_then(|pages| pages.iter().find(|p| p.asset == asset))
        {
            return respond(page, headers);
        }
        respond(
            &Page::rendered(asset, &origin_from_request(headers, uri), &self.catalogs),
            headers,
        )
    }
}

pub fn public_origin(cfg: &Config) -> Option<String> {
    if let Some(url) = &cfg.server.public_url {
        return Some(url.trim_end_matches('/').to_string());
    }
    if cfg.tls.enabled {
        return cfg.tls.domains.first().map(|d| format!("https://{d}"));
    }
    None
}

fn origin_from_request(headers: &HeaderMap, uri: &Uri) -> String {
    let host = uri
        .authority()
        .map(|a| a.as_str().to_string())
        .or_else(|| {
            headers
                .get(HOST)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        })
        .map(|h| h.to_ascii_lowercase())
        .filter(|h| is_safe_host(h))
        .unwrap_or_else(|| "localhost".into());
    let local = host.starts_with("localhost")
        || host.starts_with("127.")
        || host.starts_with("[::1]")
        || host.starts_with('[');
    let scheme = if local { "http" } else { "https" };
    format!("{scheme}://{host}")
}

fn is_safe_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 255
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'))
}

fn render(template: &str, origin: &str) -> String {
    let host = origin.split_once("://").map(|(_, h)| h).unwrap_or(origin);
    template
        .replace("{{origin}}", origin)
        .replace("{{host}}", host)
}

fn etag(body: &[u8]) -> HeaderValue {
    let hash = blake3::hash(body).to_hex();
    HeaderValue::from_str(&format!("\"{}\"", &hash[..20])).expect("hex etag")
}

fn index_csp(html: &str) -> HeaderValue {
    let script = inline_script(html).unwrap_or_default();
    let digest = ring::digest::digest(&ring::digest::SHA256, script.as_bytes());
    let hash = base64::engine::general_purpose::STANDARD.encode(digest.as_ref());
    HeaderValue::from_str(&format!(
        "default-src 'none'; script-src 'sha256-{hash}'; style-src 'unsafe-inline'; \
         font-src 'self'; img-src 'self' blob:; connect-src 'self'; base-uri 'none'; \
         form-action 'none'; frame-ancestors 'none'"
    ))
    .expect("ascii csp")
}

fn inline_script(html: &str) -> Option<&str> {
    let start = html.find("<script>")? + "<script>".len();
    let end = start + html[start..].find("</script>")?;
    Some(&html[start..end])
}

fn respond(page: &Page, headers: &HeaderMap) -> Response {
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
        Response::new(Body::from(page.body.clone()))
    };
    let h = r.headers_mut();
    h.insert(
        CONTENT_TYPE,
        HeaderValue::from_static(page.asset.content_type()),
    );
    h.insert(
        CACHE_CONTROL,
        HeaderValue::from_static(page.asset.cache_control()),
    );
    h.insert(ETAG, page.etag.clone());
    if let Some(csp) = &page.csp {
        h.insert(CONTENT_SECURITY_POLICY, csp.clone());
    }
    if page.asset.indexable() {
        h.insert(
            HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("all"),
        );
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(origin: Option<&str>) -> Site {
        let mut cfg = Config::default();
        cfg.server.public_url = origin.map(str::to_string);
        Site::new(&cfg)
    }

    fn body(r: Response) -> String {
        let bytes = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(axum::body::to_bytes(r.into_body(), usize::MAX))
            .unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[cfg(feature = "site")]
    #[test]
    fn script_hash_covers_the_inline_script() {
        let script = inline_script(page::INDEX).unwrap();
        assert!(script.contains("fetch(`/${style}/"));
        assert!(!script.contains("{{"));
        let page = Page::rendered(Asset::Index, "https://favicons.page", &[]);
        let csp = page.csp.unwrap();
        let csp = csp.to_str().unwrap();
        assert!(csp.contains("script-src 'sha256-"));
        assert!(csp.contains("font-src 'self'"));
        assert!(!csp.contains("https:"));
        assert_eq!(page::INDEX.matches("<script>").count(), 1);
    }

    #[cfg(feature = "site")]
    #[test]
    fn page_loads_nothing_from_other_origins() {
        for needle in [
            "googleapis",
            "gstatic",
            "src=\"http",
            "href=\"//",
            "@import",
        ] {
            assert!(!page::INDEX.contains(needle), "{needle}");
        }
        assert!(page::INDEX.contains("url(\"/fonts/inter.woff2\")"));
        assert!(!page::FONT_SANS.is_empty() && !page::FONT_MONO.is_empty());
    }

    #[cfg(feature = "site")]
    #[test]
    fn names_itself_after_the_configured_origin() {
        let s = site(Some("https://favicons.page/"));
        let html = body(s.serve(Asset::Index, &HeaderMap::new(), &Uri::from_static("/")));
        assert!(html.contains("<title>favicons.page"));
        assert!(html.contains(r#"<link rel="canonical" href="https://favicons.page/">"#));
        assert!(html.contains("https://favicons.page/og.png"));
        assert!(!html.contains("{{"));
    }

    #[test]
    fn docs_name_themselves_after_the_origin() {
        let s = site(Some("https://favicons.page/"));
        let robots = body(s.serve(Asset::Robots, &HeaderMap::new(), &Uri::from_static("/")));
        assert!(robots.contains("Sitemap: https://favicons.page/sitemap.xml"));
        assert!(robots.contains("Disallow: /color/"));
        let api = body(s.serve(Asset::OpenApi, &HeaderMap::new(), &Uri::from_static("/")));
        let json: serde_json::Value = serde_json::from_str(&api).unwrap();
        assert_eq!(json["servers"][0]["url"], "https://favicons.page");
    }

    #[test]
    fn falls_back_to_a_sanitized_host_header() {
        let s = site(None);
        let mut h = HeaderMap::new();
        h.insert(HOST, HeaderValue::from_static("icons.example.org"));
        let llms = body(s.serve(Asset::Llms, &h, &Uri::from_static("/llms.txt")));
        assert!(llms.starts_with("# icons.example.org"));
        h.insert(HOST, HeaderValue::from_static("evil.com\"><script>"));
        let llms = body(s.serve(Asset::Llms, &h, &Uri::from_static("/llms.txt")));
        assert!(llms.starts_with("# localhost"));
    }

    #[test]
    fn every_template_is_well_formed() {
        for asset in Asset::TEMPLATED {
            let page = Page::rendered(asset, "https://favicons.page", &["selfhst".into()]);
            let text = std::str::from_utf8(&page.body).unwrap();
            assert!(!text.contains("{{"), "{asset:?}");
        }
    }
}
