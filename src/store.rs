use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use bytes::Bytes;
use moka::future::Cache;
use object_store::path::Path;
use object_store::{Attribute, Attributes, ObjectStore, ObjectStoreExt, PutOptions, PutPayload};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::config::{Cache as CacheCfg, HttpCache};
use crate::contract::{MonoSource, OutputFormat, Slot, Theme};

pub const LAYOUT_VERSION: &str = "v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Ok,
    NotFound,
    FetchFailed,
    TooNew,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub domain: String,
    pub fetched_at: u64,
    pub status: Status,
    #[serde(default)]
    pub format: OutputFormat,
    #[serde(default)]
    pub light: Vec<u32>,
    #[serde(default)]
    pub dark: Vec<u32>,
    #[serde(default)]
    pub mono: Vec<u32>,
    #[serde(default)]
    pub mono_source: Option<MonoSource>,
    #[serde(default)]
    pub etags: BTreeMap<String, String>,
}

impl Meta {
    pub fn new(domain: &str, status: Status) -> Self {
        Self {
            domain: domain.into(),
            fetched_at: now(),
            status,
            format: OutputFormat::default(),
            light: vec![],
            dark: vec![],
            mono: vec![],
            mono_source: None,
            etags: BTreeMap::new(),
        }
    }

    pub fn sizes(&self, theme: Theme) -> &[u32] {
        match theme {
            Theme::Light => &self.light,
            Theme::Dark => &self.dark,
        }
    }

    pub fn resolve_theme(&self, wanted: Theme) -> Option<Theme> {
        [wanted, wanted.other()]
            .into_iter()
            .find(|t| !self.sizes(*t).is_empty())
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn object_name(slot: impl Into<Slot>, size: u32, format: OutputFormat) -> String {
    format!("{}-{size}.{}", slot.into().as_str(), format.ext())
}

pub struct Store {
    backend: Option<Arc<dyn ObjectStore>>,
    attributes: bool,
    prefix: Path,
    public_base: Option<String>,
    timeout: Duration,
    object_cache_control: String,
    meta_mem: Cache<String, Arc<Meta>>,
    absent: Cache<String, ()>,
    img_mem: Cache<String, Bytes>,
}

impl Store {
    pub fn new(cfg: &CacheCfg, http: &HttpCache) -> anyhow::Result<Self> {
        let (backend, prefix, attributes) = build_backend(cfg)?;
        Ok(Self {
            backend,
            attributes,
            prefix,
            public_base: cfg
                .public_base_url
                .as_ref()
                .map(|u| u.trim_end_matches('/').to_string()),
            timeout: Duration::from_millis(cfg.storage_timeout_ms),
            object_cache_control: http.icon.browser.clone(),
            meta_mem: Cache::builder()
                .max_capacity(cfg.memory_meta_entries)
                .time_to_live(Duration::from_secs(cfg.memory_ttl_secs))
                .build(),
            absent: Cache::builder()
                .max_capacity(cfg.memory_meta_entries)
                .time_to_live(Duration::from_secs(cfg.absent_ttl_secs.max(1)))
                .build(),
            img_mem: Cache::builder()
                .max_capacity(cfg.memory_max_bytes)
                .weigher(|k: &String, v: &Bytes| (k.len() + v.len()).try_into().unwrap_or(u32::MAX))
                .time_to_live(Duration::from_secs(cfg.memory_ttl_secs))
                .build(),
        })
    }

    pub fn has_backend(&self) -> bool {
        self.backend.is_some()
    }

    fn path(&self, domain: &str, name: &str) -> Path {
        let rel = format!("{LAYOUT_VERSION}/{domain}/{name}");
        if self.prefix.as_ref().is_empty() {
            Path::from(rel)
        } else {
            Path::from(format!("{}/{rel}", self.prefix))
        }
    }

    pub fn public_url(
        &self,
        domain: &str,
        slot: Slot,
        size: u32,
        format: OutputFormat,
    ) -> Option<String> {
        let base = self.public_base.as_ref()?;
        Some(format!(
            "{base}/{LAYOUT_VERSION}/{domain}/{}",
            object_name(slot, size, format)
        ))
    }

    pub async fn meta(&self, domain: &str) -> Option<Arc<Meta>> {
        if let Some(m) = self.meta_mem.get(domain).await {
            return Some(m);
        }
        let backend = self.backend.as_ref()?;
        if self.absent.contains_key(domain) {
            return None;
        }
        match self.read(backend, &self.path(domain, "meta.json")).await {
            Ok(Some(bytes)) => match serde_json::from_slice::<Meta>(&bytes) {
                Ok(meta) if meta.domain == domain => {
                    let meta = Arc::new(meta);
                    self.meta_mem.insert(domain.to_string(), meta.clone()).await;
                    Some(meta)
                }
                _ => {
                    tracing::warn!(domain, "ignoring corrupt meta.json");
                    None
                }
            },
            Ok(None) => {
                self.absent.insert(domain.to_string(), ()).await;
                None
            }
            Err(e) => {
                tracing::warn!(domain, error = %e, "storage read failed");
                None
            }
        }
    }

    pub async fn image(
        &self,
        domain: &str,
        slot: Slot,
        size: u32,
        format: OutputFormat,
    ) -> Option<Bytes> {
        let name = object_name(slot, size, format);
        let key = format!("{domain}/{name}");
        if let Some(b) = self.img_mem.get(&key).await {
            return Some(b);
        }
        let backend = self.backend.as_ref()?;
        match self.read(backend, &self.path(domain, &name)).await {
            Ok(Some(b)) => {
                self.img_mem.insert(key, b.clone()).await;
                Some(b)
            }
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(domain, error = %e, "storage read failed");
                None
            }
        }
    }

    pub async fn put(&self, meta: Meta, images: Vec<(Slot, u32, Vec<u8>)>) {
        let domain = meta.domain.clone();
        for (theme, size, data) in images {
            let name = object_name(theme, size, meta.format);
            let bytes = Bytes::from(data);
            if let Some(backend) = &self.backend {
                let opts = self.put_options(meta.format.mime(), &self.object_cache_control);
                let path = self.path(&domain, &name);
                let res = tokio::time::timeout(
                    self.timeout,
                    backend.put_opts(&path, PutPayload::from(bytes.clone()), opts),
                )
                .await;
                if !matches!(res, Ok(Ok(_))) {
                    tracing::warn!(domain, name, "storage write failed");
                }
            }
            self.img_mem.insert(format!("{domain}/{name}"), bytes).await;
        }
        if let Some(backend) = &self.backend {
            match serde_json::to_vec(&meta) {
                Ok(json) => {
                    let opts = self.put_options("application/json", "no-store");
                    let res = tokio::time::timeout(
                        self.timeout,
                        backend.put_opts(
                            &self.path(&domain, "meta.json"),
                            PutPayload::from(json),
                            opts,
                        ),
                    )
                    .await;
                    if !matches!(res, Ok(Ok(_))) {
                        tracing::warn!(domain, "storage meta write failed");
                    }
                }
                Err(e) => tracing::warn!(domain, error = %e, "meta serialization failed"),
            }
        }
        self.absent.invalidate(&domain).await;
        self.meta_mem.insert(domain, Arc::new(meta)).await;
    }

    fn put_options(&self, content_type: &str, cache_control: &str) -> PutOptions {
        if !self.attributes {
            return PutOptions::default();
        }
        let mut attrs = Attributes::new();
        attrs.insert(Attribute::ContentType, content_type.to_string().into());
        attrs.insert(Attribute::CacheControl, cache_control.to_string().into());
        PutOptions {
            attributes: attrs,
            ..Default::default()
        }
    }

    async fn read(
        &self,
        backend: &Arc<dyn ObjectStore>,
        path: &Path,
    ) -> anyhow::Result<Option<Bytes>> {
        let fut = async {
            match backend.get(path).await {
                Ok(r) => Ok(Some(r.bytes().await?)),
                Err(object_store::Error::NotFound { .. }) => Ok(None),
                Err(e) => Err(anyhow::Error::from(e)),
            }
        };
        tokio::time::timeout(self.timeout, fut)
            .await
            .context("storage timeout")?
    }
}

type Backend = (Option<Arc<dyn ObjectStore>>, Path, bool);

fn build_backend(cfg: &CacheCfg) -> anyhow::Result<Backend> {
    let spec = cfg.storage.trim();
    if spec.is_empty() || spec.eq_ignore_ascii_case("none") || spec.eq_ignore_ascii_case("memory") {
        return Ok((None, Path::default(), false));
    }
    let url = if spec.contains("://") {
        Url::parse(spec).context("cache.storage is not a valid URL")?
    } else {
        let abs = std::path::absolute(spec)?;
        Url::from_directory_path(&abs).map_err(|_| anyhow::anyhow!("bad cache directory {spec}"))?
    };
    if url.scheme() == "file" {
        let dir = url
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("bad file url"))?;
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let fs = object_store::local::LocalFileSystem::new_with_prefix(&dir)?
            .with_automatic_cleanup(true);
        return Ok((Some(Arc::new(fs)), Path::default(), false));
    }
    let attributes = !matches!(url.scheme(), "memory");
    let (store, prefix) = object_store::parse_url_opts(&url, cfg.options.iter())?;
    Ok((Some(Arc::from(store)), prefix, attributes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &std::path::Path) -> Store {
        let cfg = CacheCfg {
            storage: format!("file://{}", dir.display()),
            ..CacheCfg::default()
        };
        Store::new(&cfg, &HttpCache::default()).unwrap()
    }

    #[tokio::test]
    async fn round_trips_through_filesystem() {
        let dir = std::env::temp_dir().join(format!("favicons-test-{}", rand::random::<u64>()));
        let store = store_in(&dir);
        assert!(store.meta("example.com").await.is_none());
        let mut meta = Meta::new("example.com", Status::Ok);
        meta.light = vec![16, 32];
        store
            .put(
                meta,
                vec![
                    (Slot::Light, 16, vec![1, 2, 3]),
                    (Slot::Mono, 32, vec![4, 5]),
                ],
            )
            .await;

        let fresh = store_in(&dir);
        let meta = fresh.meta("example.com").await.unwrap();
        assert_eq!(meta.light, [16, 32]);
        assert_eq!(
            fresh
                .image("example.com", Slot::Mono, 32, OutputFormat::Webp)
                .await
                .unwrap()
                .as_ref(),
            [4, 5]
        );
        assert!(dir.join("v1/example.com/light-16.webp").exists());
        assert!(dir.join("v1/example.com/mono-32.webp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn resolves_theme_with_fallback() {
        let mut m = Meta::new("e.com", Status::Ok);
        m.light = vec![16];
        assert_eq!(m.resolve_theme(Theme::Dark), Some(Theme::Light));
        m.dark = vec![32];
        assert_eq!(m.resolve_theme(Theme::Dark), Some(Theme::Dark));
        assert_eq!(
            Meta::new("e.com", Status::NotFound).resolve_theme(Theme::Light),
            None
        );
    }
}
