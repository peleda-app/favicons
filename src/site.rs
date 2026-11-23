use std::sync::Arc;

use axum::body::Body;
use axum::response::Response;
use bytes::Bytes;
use http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, HOST, IF_NONE_MATCH};
use http::{HeaderMap, HeaderValue, StatusCode, Uri};

use crate::config::Config;

const ROBOTS: &str = include_str!("../site/robots.txt");
const LLMS: &str = include_str!("../site/llms.txt");

const SITEMAP: &str = include_str!("../site/sitemap.xml");
pub const DOCS: &str = "https://favicons.page/docs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    Root,
    Robots,
    Sitemap,
    Llms,
    OpenApi,
}

impl Asset {
    const TEMPLATED: [Asset; 5] = [
        Asset::Root,
        Asset::Robots,
        Asset::Sitemap,
        Asset::Llms,
        Asset::OpenApi,
    ];

    fn template(self) -> Option<&'static str> {
        match self {
            Asset::Robots => Some(ROBOTS),
            Asset::Sitemap => Some(SITEMAP),
            Asset::Llms => Some(LLMS),
            Asset::Root | Asset::OpenApi => None,
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Asset::Robots | Asset::Llms => "text/plain; charset=utf-8",
            Asset::Sitemap => "application/xml; charset=utf-8",
            Asset::Root | Asset::OpenApi => "application/json",
        }
    }

    fn cache_control(self) -> &'static str {
        match self {
            Asset::Root => "public, max-age=300, stale-while-revalidate=86400",
            _ => "public, max-age=3600, stale-while-revalidate=86400",
        }
    }
}

pub struct Page {
    body: Bytes,
    asset: Asset,
    etag: HeaderValue,
}

impl Page {
    fn new(asset: Asset, body: Bytes) -> Self {
        Self {
            etag: etag(&body),
            body,
            asset,
        }
    }

    fn rendered(asset: Asset, origin: &str, catalogs: &[String]) -> Self {
        match asset {
            Asset::OpenApi => {
                let spec = crate::openapi::Spec { origin, catalogs };
                return Page::new(asset, Bytes::from(spec.render()));
            }
            Asset::Root => return Page::new(asset, Bytes::from(root(origin))),
            _ => {}
        }
        let template = asset.template().unwrap_or_default();
        Page::new(asset, Bytes::from(render(template, origin)))
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
    let studio = if host == "favicons.page" {
        format!(
            "  <url>\n    <loc>{origin}/</loc>\n    <changefreq>weekly</changefreq>\n  </url>\n  <url>\n    <loc>{origin}/docs/</loc>\n    <changefreq>weekly</changefreq>\n  </url>\n"
        )
    } else {
        String::new()
    };
    template
        .replace("{{studio}}", &studio)
        .replace("{{origin}}", origin)
        .replace("{{host}}", host)
}

fn root(origin: &str) -> String {
    let body = serde_json::json!({
        "name": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
        "docs": DOCS,
        "openapi": format!("{origin}/openapi.json"),
        "llms": format!("{origin}/llms.txt"),
        "example": format!("{origin}/color/github.com"),
        "source": env!("CARGO_PKG_REPOSITORY"),
    });
    serde_json::to_string_pretty(&body).unwrap_or_default()
}

fn etag(body: &[u8]) -> HeaderValue {
    let hash = blake3::hash(body).to_hex();
    HeaderValue::from_str(&format!("\"{}\"", &hash[..20])).expect("hex etag")
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

    #[test]
    fn root_points_at_the_docs() {
        let s = site(Some("https://icons.example.org"));
        let json: serde_json::Value = serde_json::from_str(&body(s.serve(
            Asset::Root,
            &HeaderMap::new(),
            &Uri::from_static("/"),
        )))
        .unwrap();
        assert_eq!(json["name"], "favicons");
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["docs"], DOCS);
        assert_eq!(json["openapi"], "https://icons.example.org/openapi.json");
    }

    #[test]
    fn docs_name_themselves_after_the_origin() {
        let s = site(Some("https://favicons.page/"));
        let robots = body(s.serve(Asset::Robots, &HeaderMap::new(), &Uri::from_static("/")));
        assert!(robots.contains("Sitemap: https://favicons.page/sitemap.xml"));
        assert!(robots.contains("Disallow: /color/"));
        let map = body(s.serve(Asset::Sitemap, &HeaderMap::new(), &Uri::from_static("/")));
        assert!(map.contains("<loc>https://favicons.page/openapi.json</loc>"));
        assert!(map.contains("<loc>https://favicons.page/</loc>"));
        assert!(map.contains("<loc>https://favicons.page/docs/</loc>"));
        let other = site(Some("https://icons.example.org"));
        let map = body(other.serve(Asset::Sitemap, &HeaderMap::new(), &Uri::from_static("/")));
        assert!(!map.contains("/docs/"));
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
