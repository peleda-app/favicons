use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use bytes::Bytes;
use http::HeaderValue;
use moka::sync::Cache;
use object_store::path::Path;
use object_store::{Attribute, Attributes, ObjectStore, ObjectStoreExt, PutOptions, PutPayload};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::config::{Cache as CacheCfg, HttpCache};
use crate::contract::{MonoSource, Provenance, SVG_MIME, Slot};
use crate::palette::Palette;
use crate::vector::VectorOrigin;

pub const LAYOUT_VERSION: &str = "v3";

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
    pub color: Option<VectorOrigin>,
    #[serde(default)]
    pub mono: Option<VectorOrigin>,
    #[serde(default)]
    pub mono_source: Option<MonoSource>,
    #[serde(default)]
    pub upstream: Option<String>,
    #[serde(default)]
    pub provenance: Option<Provenance>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub etags: BTreeMap<String, String>,
    #[serde(default)]
    pub palette: Option<Palette>,
}

impl Meta {
    pub fn new(domain: &str, status: Status) -> Self {
        Self {
            domain: domain.into(),
            fetched_at: now(),
            status,
            color: None,
            mono: None,
            mono_source: None,
            upstream: None,
            provenance: None,
            license: None,
            etags: BTreeMap::new(),
            palette: None,
        }
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn object_name(slot: Slot) -> &'static str {
    match slot {
        Slot::Color => "color.svg",
        Slot::Mono => "mono.svg",
    }
}

pub struct Icon {
    pub data: Bytes,
    pub etag: Option<HeaderValue>,
    pub domain: Option<HeaderValue>,
    pub palette: crate::palette::Headers,
}

impl Icon {
    pub fn new(
        data: Bytes,
        etag: Option<&str>,
        domain: Option<&str>,
        palette: Option<Palette>,
    ) -> Arc<Self> {
        Arc::new(Self::build(data, etag, domain, palette))
    }

    fn build(
        data: Bytes,
        etag: Option<&str>,
        domain: Option<&str>,
        palette: Option<Palette>,
    ) -> Self {
        Self {
            data,
            etag: etag.and_then(|e| HeaderValue::from_str(&format!("\"{e}\"")).ok()),
            domain: domain.and_then(|d| HeaderValue::from_str(d).ok()),
            palette: palette.map(|p| p.headers()).unwrap_or_default(),
        }
    }

    fn of(meta: &Meta, slot: Slot, data: Bytes) -> Arc<Self> {
        Arc::new(Self::build(
            data,
            meta.etags.get(object_name(slot)).map(String::as_str),
            Some(&meta.domain),
            meta.palette,
        ))
    }
}

type IconCache = Cache<String, Arc<Icon>>;

fn icon_cache(cfg: &CacheCfg) -> IconCache {
    Cache::builder()
        .max_capacity(cfg.memory_max_bytes / 2)
        .weigher(|k: &String, v: &Arc<Icon>| {
            (k.len() + v.data.len()).try_into().unwrap_or(u32::MAX)
        })
        .time_to_live(Duration::from_secs(cfg.memory_ttl_secs))
        .build()
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
    img_color: IconCache,
    img_mono: IconCache,
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
            img_color: icon_cache(cfg),
            img_mono: icon_cache(cfg),
        })
    }

    fn icons(&self, slot: Slot) -> &IconCache {
        match slot {
            Slot::Color => &self.img_color,
            Slot::Mono => &self.img_mono,
        }
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

    pub fn public_url(&self, domain: &str, slot: Slot) -> Option<String> {
        let base = self.public_base.as_ref()?;
        Some(format!(
            "{base}/{LAYOUT_VERSION}/{domain}/{}",
            object_name(slot)
        ))
    }

    pub async fn meta(&self, domain: &str) -> Option<Arc<Meta>> {
        if let Some(m) = self.meta_mem.get(domain) {
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
                    self.meta_mem.insert(domain.to_string(), meta.clone());
                    Some(meta)
                }
                _ => {
                    tracing::warn!(domain, "ignoring corrupt meta.json");
                    None
                }
            },
            Ok(None) => {
                self.absent.insert(domain.to_string(), ());
                None
            }
            Err(e) => {
                tracing::warn!(domain, error = %e, "storage read failed");
                None
            }
        }
    }

    pub async fn image(&self, meta: &Meta, slot: Slot) -> Option<Arc<Icon>> {
        let domain = meta.domain.as_str();
        let cache = self.icons(slot);
        if let Some(icon) = cache.get(domain) {
            return Some(icon);
        }
        let backend = self.backend.as_ref()?;
        match self
            .read(backend, &self.path(domain, object_name(slot)))
            .await
        {
            Ok(Some(b)) => {
                let icon = Icon::of(meta, slot, b);
                cache.insert(domain.to_string(), icon.clone());
                Some(icon)
            }
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(domain, error = %e, "storage read failed");
                None
            }
        }
    }

    pub async fn put(&self, meta: Meta, images: Vec<(Slot, Vec<u8>)>) {
        let domain = meta.domain.clone();
        for (slot, data) in images {
            let name = object_name(slot);
            let bytes = Bytes::from(data);
            if let Some(backend) = &self.backend {
                let opts = self.put_options(SVG_MIME, &self.object_cache_control);
                let path = self.path(&domain, name);
                let res = tokio::time::timeout(
                    self.timeout,
                    backend.put_opts(&path, PutPayload::from(bytes.clone()), opts),
                )
                .await;
                if !matches!(res, Ok(Ok(_))) {
                    tracing::warn!(domain, name, "storage write failed");
                }
            }
            self.icons(slot)
                .insert(domain.clone(), Icon::of(&meta, slot, bytes));
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
        self.absent.invalidate(&domain);
        self.meta_mem.insert(domain, Arc::new(meta));
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
        meta.color = Some(VectorOrigin::Traced);
        store
            .put(
                meta,
                vec![(Slot::Color, vec![1, 2, 3]), (Slot::Mono, vec![4, 5])],
            )
            .await;

        let fresh = store_in(&dir);
        let meta = fresh.meta("example.com").await.unwrap();
        assert_eq!(meta.color, Some(VectorOrigin::Traced));
        assert_eq!(
            fresh.image(&meta, Slot::Mono).await.unwrap().data.as_ref(),
            [4, 5]
        );
        assert!(dir.join("v3/example.com/color.svg").exists());
        assert!(dir.join("v3/example.com/mono.svg").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
