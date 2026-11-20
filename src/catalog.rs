use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::Deserialize;
use url::Url;

use crate::config::TheSvg;
use crate::domain::normalize;
use crate::net::{HttpClient, read_source};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub slug: String,
    pub color: Url,
    pub mono: Option<Url>,
    pub license: String,
}

#[derive(Deserialize)]
struct RawEntry {
    slug: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    license: String,
    #[serde(default)]
    collection: String,
    #[serde(default)]
    variants: HashMap<String, String>,
}

pub struct Catalog {
    cfg: TheSvg,
    index: RwLock<Arc<HashMap<String, Entry>>>,
}

impl Catalog {
    pub fn new(cfg: &TheSvg) -> Option<Self> {
        cfg.enabled.then(|| Self {
            cfg: cfg.clone(),
            index: RwLock::new(Arc::new(HashMap::new())),
        })
    }

    pub fn lookup(&self, domain: &str) -> Option<Entry> {
        self.index.read().ok()?.get(domain).cloned()
    }

    pub fn len(&self) -> usize {
        self.index.read().map_or(0, |i| i.len())
    }

    pub async fn reload(&self, client: &HttpClient) {
        let index = match read_source(client, &self.cfg.index_url, self.cfg.max_index_bytes).await {
            Ok(bytes) => match build_index(&bytes, &self.cfg) {
                Ok(index) => index,
                Err(e) => {
                    tracing::warn!(error = %e, "thesvg index rejected; keeping previous one");
                    return;
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "thesvg index download failed; keeping previous one");
                return;
            }
        };
        tracing::info!(entries = index.len(), "thesvg catalog loaded");
        if let Ok(mut g) = self.index.write() {
            *g = Arc::new(index);
        }
    }
}

fn asset(base: &Url, path: &str) -> Option<Url> {
    let path = path.strip_prefix('/')?;
    let safe = path.starts_with("icons/")
        && path.ends_with(".svg")
        && !path.contains("..")
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b));
    if !safe {
        return None;
    }
    let url = base.join(path).ok()?;
    (url.scheme() == "https").then_some(url)
}

pub fn build_index(json: &[u8], cfg: &TheSvg) -> anyhow::Result<HashMap<String, Entry>> {
    let raw: Vec<RawEntry> = serde_json::from_slice(json)?;
    let mut base = cfg.asset_base.clone();
    if !base.ends_with('/') {
        base.push('/');
    }
    let base = Url::parse(&base)?;
    let mut index: HashMap<String, Entry> = HashMap::new();
    for e in raw {
        if !cfg.collections.contains(&e.collection) || !cfg.licenses.contains(&e.license) {
            continue;
        }
        let Some(site) = e.url.as_deref().and_then(|u| Url::parse(u).ok()) else {
            continue;
        };
        if !matches!(site.path(), "" | "/") || site.query().is_some() {
            continue;
        }
        let Some(host) = site
            .host_str()
            .map(|h| h.trim_start_matches("www.").to_ascii_lowercase())
        else {
            continue;
        };
        let Some(n) = normalize(&host, true, &[]) else {
            continue;
        };
        if n.registrable != host {
            continue;
        }
        let colour = e
            .variants
            .get("color")
            .or_else(|| e.variants.get("default"));
        let Some(color) = colour.and_then(|p| asset(&base, p)) else {
            continue;
        };
        let mono = e.variants.get("mono").and_then(|p| asset(&base, p));
        let entry = Entry {
            slug: e.slug,
            color,
            mono,
            license: e.license,
        };
        let label = host.split('.').next().unwrap_or_default().to_string();
        match index.get(&host) {
            Some(existing) if existing.slug == label || entry.slug != label => {}
            _ => {
                index.insert(host, entry);
            }
        }
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> TheSvg {
        TheSvg {
            enabled: true,
            ..TheSvg::default()
        }
    }

    #[test]
    fn indexes_only_root_urls_with_allowed_licenses() {
        let json = br#"[
          {"slug":"github","url":"https://github.com/","license":"CC0-1.0","collection":"brands","variants":{"default":"/icons/github/default.svg","mono":"/icons/github/mono.svg"}},
          {"slug":"github-copilot","url":"https://github.com/","license":"CC0-1.0","collection":"brands","variants":{"default":"/icons/github-copilot/default.svg"}},
          {"slug":"gmail","url":"https://mail.google.com/","license":"CC0-1.0","collection":"brands","variants":{"default":"/icons/gmail/default.svg"}},
          {"slug":"aws-s3","url":"https://aws.amazon.com/s3/","license":"CC0-1.0","collection":"brands","variants":{"default":"/icons/aws-s3/default.svg"}},
          {"slug":"spotify","url":"https://www.spotify.com/","license":"CC0-1.0","collection":"brands","variants":{"color":"/icons/spotify/color.svg","default":"/icons/spotify/default.svg"}},
          {"slug":"msft","url":"https://microsoft.com/","license":"Microsoft proprietary","collection":"brands","variants":{"default":"/icons/msft/default.svg"}},
          {"slug":"login","url":"https://login.example/","license":"CC0-1.0","collection":"auth-badges","variants":{"default":"/icons/login/default.svg"}},
          {"slug":"evil","url":"https://evil.com/","license":"CC0-1.0","collection":"brands","variants":{"default":"/../../etc/passwd"}}
        ]"#;
        let index = build_index(json, &cfg()).unwrap();
        let mut keys: Vec<_> = index.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["github.com", "spotify.com"]);
        let gh = &index["github.com"];
        assert_eq!(gh.slug, "github", "the slug matching the domain wins");
        assert!(
            gh.color
                .as_str()
                .ends_with("/public/icons/github/default.svg")
        );
        assert!(
            gh.mono
                .as_ref()
                .unwrap()
                .as_str()
                .ends_with("/icons/github/mono.svg")
        );
        assert!(
            index["spotify.com"]
                .color
                .as_str()
                .ends_with("/icons/spotify/color.svg")
        );
    }
}
