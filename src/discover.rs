use std::collections::HashSet;

use base64::Engine;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::contract::Theme;
use crate::net::is_fetchable_url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Scope {
    pub allow_http: bool,
    pub max_inline: usize,
}

impl Scope {
    pub fn allows(&self, url: &Url) -> bool {
        is_fetchable_url(url, self.allow_http)
    }
}

const MAX_LINKS: usize = 64;
const MAX_LINK_TAGS: usize = 1024;
pub const SIZE_ANY: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    Remote(Url),
    Inline(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub location: Location,
    pub declared: u32,
    pub svg: bool,
    pub theme: Option<Theme>,
    pub mono: bool,
}

impl Candidate {
    pub fn score(&self) -> u32 {
        match (self.svg, self.declared) {
            (true, _) | (_, SIZE_ANY) => 10_000,
            (false, d) if d >= 128 => 20_000 - d.abs_diff(256).min(9_999),
            (false, d) => d,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub icons: Vec<Candidate>,
    pub manifest: Option<Url>,
}

pub fn parse_html(html: &[u8], page_url: &Url, scope: &Scope) -> Page {
    let head = cut_at_head_end(html);
    let doc = Html::parse_document(&String::from_utf8_lossy(head));
    let (Ok(base_sel), Ok(link_sel)) = (
        Selector::parse("base[href]"),
        Selector::parse("link[rel][href]"),
    ) else {
        return Page::default();
    };
    let base = doc
        .select(&base_sel)
        .next()
        .and_then(|b| b.value().attr("href"))
        .and_then(|h| page_url.join(h.trim()).ok())
        .filter(|u| scope.allows(u))
        .unwrap_or_else(|| page_url.clone());

    let mut page = Page::default();
    let mut relevant = 0;
    for link in doc.select(&link_sel).take(MAX_LINK_TAGS) {
        if relevant >= MAX_LINKS {
            break;
        }
        let el = link.value();
        let rel = el.attr("rel").unwrap_or_default().to_ascii_lowercase();
        let tokens: Vec<&str> = rel.split_ascii_whitespace().collect();
        let href = el.attr("href").unwrap_or_default().trim();
        if !tokens
            .iter()
            .any(|t| *t == "manifest" || t.contains("icon"))
        {
            continue;
        }
        relevant += 1;
        if tokens.contains(&"manifest") {
            if page.manifest.is_none() {
                page.manifest = base.join(href).ok().filter(|u| scope.allows(u));
            }
            continue;
        }
        if tokens.contains(&"mask-icon") {
            if let Some(location) = resolve(&base, href, scope) {
                page.icons.push(Candidate {
                    location,
                    declared: SIZE_ANY,
                    svg: true,
                    theme: None,
                    mono: true,
                });
            }
            continue;
        }
        let apple = tokens.iter().any(|t| t.starts_with("apple-touch-icon"));
        if !apple && !tokens.contains(&"icon") {
            continue;
        }
        let Some(location) = resolve(&base, href, scope) else {
            continue;
        };
        let mime = el.attr("type").unwrap_or_default().to_ascii_lowercase();
        let svg = mime.contains("svg") || looks_like_svg_path(&location);
        let declared = match el.attr("sizes").map(parse_sizes) {
            Some(Some(n)) => n,
            _ if svg => SIZE_ANY,
            _ if apple => 180,
            _ => 0,
        };
        let theme = el.attr("media").and_then(parse_media_theme);
        page.icons.push(Candidate {
            location,
            declared,
            svg,
            theme,
            mono: false,
        });
    }
    page
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    icons: Vec<ManifestIcon>,
}

#[derive(Deserialize)]
struct ManifestIcon {
    src: String,
    #[serde(default)]
    sizes: Option<String>,
    #[serde(default, rename = "type")]
    mime: Option<String>,
    #[serde(default)]
    purpose: Option<String>,
}

pub fn parse_manifest(json: &[u8], manifest_url: &Url, scope: &Scope) -> Vec<Candidate> {
    let Ok(m) = serde_json::from_slice::<Manifest>(json) else {
        return vec![];
    };
    m.icons
        .into_iter()
        .take(MAX_LINKS)
        .flat_map(|icon| {
            let purpose = icon
                .purpose
                .unwrap_or_else(|| "any".into())
                .to_ascii_lowercase();
            let purposes: Vec<&str> = purpose.split_ascii_whitespace().collect();
            let color = purposes.iter().any(|p| *p == "any" || *p == "maskable");
            let mono = purposes.contains(&"monochrome");
            let Some(location) = resolve(manifest_url, icon.src.trim(), scope) else {
                return vec![];
            };
            let svg = icon.mime.as_deref().is_some_and(|t| t.contains("svg"))
                || looks_like_svg_path(&location);
            let mut declared = icon
                .sizes
                .as_deref()
                .and_then(parse_sizes)
                .unwrap_or(if svg { SIZE_ANY } else { 0 });
            let mut out = Vec::new();
            if mono {
                out.push(Candidate {
                    location: location.clone(),
                    declared,
                    svg,
                    theme: None,
                    mono: true,
                });
            }
            if color {
                if !purposes.contains(&"any") && declared != SIZE_ANY {
                    declared /= 2;
                }
                out.push(Candidate {
                    location,
                    declared,
                    svg,
                    theme: None,
                    mono: false,
                });
            }
            out
        })
        .collect()
}

pub fn default_candidates(origin: &Url) -> Vec<Candidate> {
    ["/apple-touch-icon.png", "/favicon.ico"]
        .iter()
        .zip([180, 0])
        .filter_map(|(path, declared)| {
            Some(Candidate {
                location: Location::Remote(origin.join(path).ok()?),
                declared,
                svg: false,
                theme: None,
                mono: false,
            })
        })
        .collect()
}

pub const GUESSED_MANIFEST_PATHS: [&str; 3] = [
    "/manifest.json",
    "/site.webmanifest",
    "/manifest.webmanifest",
];

pub fn guessed_manifests(origin: &Url) -> Vec<Url> {
    GUESSED_MANIFEST_PATHS
        .iter()
        .filter_map(|p| origin.join(p).ok())
        .collect()
}

pub fn has_large(cands: &[Candidate]) -> bool {
    cands
        .iter()
        .any(|c| !c.mono && c.declared >= 128 && c.declared != SIZE_ANY)
}

pub fn looks_like_json_object(bytes: &[u8]) -> bool {
    let start = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    start
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == b'{')
}

pub fn rank_mono(cands: &[Candidate]) -> Vec<Candidate> {
    let mut seen = HashSet::new();
    let mut out: Vec<Candidate> = cands
        .iter()
        .filter(|c| c.mono && seen.insert(c.location.clone()))
        .cloned()
        .collect();
    out.sort_by_key(|c| (!c.svg, std::cmp::Reverse(c.declared)));
    out
}

pub fn rank(cands: Vec<Candidate>, theme: Theme) -> Vec<Candidate> {
    let mut seen = HashSet::new();
    let mut out: Vec<Candidate> = cands
        .into_iter()
        .filter(|c| !c.mono)
        .filter(|c| match theme {
            Theme::Light => c.theme != Some(Theme::Dark),
            Theme::Dark => c.theme == Some(Theme::Dark),
        })
        .filter(|c| seen.insert(c.location.clone()))
        .collect();
    out.sort_by_key(|c| (c.theme.is_none(), std::cmp::Reverse(c.score())));
    out
}

fn resolve(base: &Url, href: &str, scope: &Scope) -> Option<Location> {
    if href.is_empty() {
        return None;
    }
    if href.len() > 5 && href[..5].eq_ignore_ascii_case("data:") {
        return decode_data_url(href, scope.max_inline).map(Location::Inline);
    }
    let url = base.join(href).ok()?;
    scope.allows(&url).then_some(Location::Remote(url))
}

fn decode_data_url(href: &str, max: usize) -> Option<Vec<u8>> {
    if href.len() > max.saturating_mul(2) {
        return None;
    }
    let (meta, payload) = href[5..].split_once(',')?;
    let bytes = if meta.to_ascii_lowercase().ends_with(";base64") {
        let cleaned: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
        base64::engine::general_purpose::STANDARD
            .decode(cleaned)
            .ok()?
    } else {
        percent_encoding::percent_decode_str(payload).collect()
    };
    (bytes.len() <= max).then_some(bytes)
}

fn looks_like_svg_path(loc: &Location) -> bool {
    match loc {
        Location::Remote(u) => u.path().to_ascii_lowercase().ends_with(".svg"),
        Location::Inline(b) => crate::render::sniff_svg(b),
    }
}

pub fn parse_sizes(s: &str) -> Option<u32> {
    let s = s.trim().to_ascii_lowercase();
    if s.split_ascii_whitespace().any(|t| t == "any") {
        return Some(SIZE_ANY);
    }
    s.split_ascii_whitespace()
        .filter_map(|t| {
            let (w, h) = t.split_once('x')?;
            Some(w.parse::<u32>().ok()?.max(h.parse::<u32>().ok()?))
        })
        .max()
}

fn parse_media_theme(media: &str) -> Option<Theme> {
    let m = media.to_ascii_lowercase();
    if !m.contains("prefers-color-scheme") {
        return None;
    }
    if m.contains("dark") {
        Some(Theme::Dark)
    } else if m.contains("light") {
        Some(Theme::Light)
    } else {
        None
    }
}

fn cut_at_head_end(html: &[u8]) -> &[u8] {
    const NEEDLE: &[u8] = b"</head";
    html.windows(NEEDLE.len())
        .position(|w| w.eq_ignore_ascii_case(NEEDLE))
        .map_or(html, |p| &html[..p])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Url {
        Url::parse("https://example.com/some/page").unwrap()
    }

    fn scope(max_inline: usize) -> Scope {
        Scope {
            allow_http: true,
            max_inline,
        }
    }

    #[test]
    fn keeps_foreign_assets_but_drops_unsafe_urls() {
        let html = br#"<head>
            <base href="http://10.0.0.1/">
            <link rel="icon" href="https://assets.othercdn.com/b.png" sizes="512x512">
            <link rel="icon" href="https://169.254.169.254/c.png">
            <link rel="icon" href="https://cdn.example.com:8443/d.png">
            <link rel="icon" href="rel.png">
            </head>"#;
        let p = parse_html(html, &page(), &scope(1024));
        let urls: Vec<String> = p
            .icons
            .iter()
            .filter_map(|c| match &c.location {
                Location::Remote(u) => Some(u.to_string()),
                Location::Inline(_) => None,
            })
            .collect();
        assert_eq!(
            urls,
            [
                "https://assets.othercdn.com/b.png",
                "https://example.com/some/rel.png"
            ]
        );
    }

    #[test]
    fn extracts_icons_with_sizes_and_themes() {
        let html = br#"<!doctype html><html><head>
            <link rel="icon" href="/favicon-32.png" sizes="32x32">
            <link rel="icon" href="/dark.svg" type="image/svg+xml" media="(prefers-color-scheme: dark)">
            <link rel="apple-touch-icon" href="touch.png">
            <link rel="manifest" href="/site.webmanifest">
            <link rel="icon" href="http://127.0.0.1/internal.png">
            <link rel="icon" href="https://example.com:6379/redis.png">
            <link rel="mask-icon" href="/mask.svg">
            </head><body><link rel="icon" href="/ignored-body.png"></body></html>"#;
        let p = parse_html(html, &page(), &scope(1024));
        assert_eq!(
            p.manifest.unwrap().as_str(),
            "https://example.com/site.webmanifest"
        );
        assert_eq!(p.icons.len(), 4);
        assert!(p.icons[3].mono && p.icons[3].svg);
        assert_eq!(rank_mono(&p.icons).len(), 1);
        assert!(!rank(p.icons.clone(), Theme::Light).iter().any(|c| c.mono));
        assert_eq!(p.icons[0].declared, 32);
        assert!(p.icons[1].svg);
        assert_eq!(p.icons[1].theme, Some(Theme::Dark));
        assert_eq!(p.icons[2].declared, 180);
        assert_eq!(
            p.icons[2].location,
            Location::Remote(Url::parse("https://example.com/some/touch.png").unwrap())
        );
    }

    #[test]
    fn finds_icons_after_many_other_links() {
        let mut html = String::from("<head>");
        for i in 0..200 {
            html.push_str(&format!(r#"<link rel="preload" href="/asset{i}.js">"#));
        }
        html.push_str(r#"<link rel="icon" href="/late.png" sizes="64x64"><link rel="mask-icon" href="/pin.svg"><link rel="manifest" href="/m.json"></head>"#);
        let p = parse_html(html.as_bytes(), &page(), &scope(0));
        assert_eq!(p.icons.len(), 2);
        assert!(p.icons[1].mono);
        assert!(p.manifest.is_some());
    }

    #[test]
    fn decodes_inline_data_urls_with_limit() {
        let html = br#"<head><link rel="icon" href="data:image/png;base64,iVBORw0KGgo="></head>"#;
        let p = parse_html(html, &page(), &scope(1024));
        assert_eq!(
            p.icons[0].location,
            Location::Inline(b"\x89PNG\r\n\x1a\n".to_vec())
        );
        let p = parse_html(html, &page(), &scope(4));
        assert!(p.icons.is_empty());
    }

    #[test]
    fn ranks_large_raster_over_svg_over_small() {
        let mk = |d: u32, svg: bool, theme| Candidate {
            location: Location::Remote(Url::parse(&format!("https://e.com/{d}{svg}")).unwrap()),
            declared: d,
            svg,
            theme,
            mono: false,
        };
        let ranked = rank(
            vec![
                mk(16, false, None),
                mk(SIZE_ANY, true, None),
                mk(192, false, None),
                mk(64, false, Some(Theme::Dark)),
            ],
            Theme::Light,
        );
        let ds: Vec<_> = ranked.iter().map(|c| c.declared).collect();
        assert_eq!(ds, [192, SIZE_ANY, 16]);
        let dark = rank(
            vec![mk(64, false, Some(Theme::Dark)), mk(16, false, None)],
            Theme::Dark,
        );
        assert_eq!(dark.len(), 1);
    }

    #[test]
    fn parses_manifest() {
        let json = br#"{"icons":[{"src":"/a.png","sizes":"192x192"},{"src":"/m.png","sizes":"512x512","purpose":"maskable"},{"src":"/mono.svg","purpose":"monochrome"}]}"#;
        let c = parse_manifest(
            json,
            &Url::parse("https://example.com/m.json").unwrap(),
            &scope(0),
        );
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].declared, 192);
        assert_eq!(c[1].declared, 256);
        assert!(c[2].mono && c[2].svg);
    }

    #[test]
    fn guesses_manifest_paths_on_origin() {
        let urls = guessed_manifests(&Url::parse("https://www.example.com/deep/page?x=1").unwrap());
        let got: Vec<_> = urls.iter().map(Url::as_str).collect();
        assert_eq!(
            got,
            [
                "https://www.example.com/manifest.json",
                "https://www.example.com/site.webmanifest",
                "https://www.example.com/manifest.webmanifest"
            ]
        );
    }

    #[test]
    fn distinguishes_json_from_spa_html() {
        assert!(looks_like_json_object(b"  \n{\"icons\":[]}"));
        assert!(looks_like_json_object(b"\xef\xbb\xbf{}"));
        assert!(!looks_like_json_object(b"<!doctype html><html>"));
        assert!(!looks_like_json_object(b"[1,2]"));
        assert!(!looks_like_json_object(b""));
    }

    #[test]
    fn detects_large_candidates() {
        let mk = |declared| Candidate {
            location: Location::Inline(vec![]),
            declared,
            svg: false,
            theme: None,
            mono: false,
        };
        assert!(!has_large(&[mk(32), mk(SIZE_ANY)]));
        assert!(has_large(&[mk(32), mk(192)]));
    }

    #[test]
    fn parses_sizes_attr() {
        assert_eq!(parse_sizes("16x16 32x32"), Some(32));
        assert_eq!(parse_sizes("any"), Some(SIZE_ANY));
        assert_eq!(parse_sizes("192X192"), Some(192));
        assert_eq!(parse_sizes("huge"), None);
    }
}
