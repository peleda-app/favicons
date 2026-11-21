use std::collections::BTreeMap;
use std::path::PathBuf;
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
use crate::disk::Disk;
use crate::palette::Palette;
use crate::vector::VectorOrigin;

pub const LAYOUT_VERSION: &str = "v4";

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
    #[serde(default)]
    pub palette_rev: u32,
    #[serde(default)]
    pub mono_rev: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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
            palette_rev: 0,
            mono_rev: 0,
            source: None,
        }
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn drop_old_layouts(root: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let layout = name
            .strip_prefix('v')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if !layout || name == LAYOUT_VERSION || !entry.path().is_dir() {
            continue;
        }
        match std::fs::remove_dir_all(entry.path()) {
            Ok(()) => tracing::info!(layout = %name, "dropped an old cache layout"),
            Err(e) => {
                tracing::warn!(layout = %name, error = %e, "could not drop an old cache layout")
            }
        }
    }
}

pub fn object_name(slot: Slot) -> &'static str {
    match slot {
        Slot::Color => "color.svg",
        Slot::Mono => "mono.svg",
    }
}

fn file_name(slot: Slot, gzipped: bool) -> &'static str {
    match (slot, gzipped) {
        (Slot::Color, false) => "color.svg",
        (Slot::Mono, false) => "mono.svg",
        (Slot::Color, true) => "color.svg.gz",
        (Slot::Mono, true) => "mono.svg.gz",
    }
}

pub struct Icon {
    body: Bytes,
    pub gzipped: bool,
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
        Arc::new(Self::build(data, false, etag, domain, palette))
    }

    fn build(
        body: Bytes,
        gzipped: bool,
        etag: Option<&str>,
        domain: Option<&str>,
        palette: Option<Palette>,
    ) -> Self {
        Self {
            body,
            gzipped,
            etag: etag.and_then(|e| HeaderValue::from_str(&format!("\"{e}\"")).ok()),
            domain: domain.and_then(|d| HeaderValue::from_str(d).ok()),
            palette: palette.map(|p| p.headers()).unwrap_or_default(),
        }
    }

    fn of(meta: &Meta, slot: Slot, body: Bytes, gzipped: bool) -> Arc<Self> {
        Arc::new(Self::build(
            body,
            gzipped,
            meta.etags.get(object_name(slot)).map(String::as_str),
            Some(&meta.domain),
            meta.palette,
        ))
    }

    pub fn raw(&self) -> Bytes {
        if self.gzipped {
            gunzip(&self.body).unwrap_or_default()
        } else {
            self.body.clone()
        }
    }

    pub fn gzip(&self) -> Option<&Bytes> {
        self.gzipped.then_some(&self.body)
    }

    pub fn size(&self) -> usize {
        self.body.len()
    }

    pub fn derived(&self, raw: Bytes, etag: Option<HeaderValue>) -> Self {
        let body = if self.gzipped { gzip(&raw) } else { raw };
        Self {
            body,
            gzipped: self.gzipped,
            etag,
            domain: self.domain.clone(),
            palette: self.palette.clone(),
        }
    }

    pub fn gzip_etag(&self) -> Option<HeaderValue> {
        let e = self.etag.as_ref()?.to_str().ok()?.trim_matches('"');
        HeaderValue::from_str(&format!("\"{e}-gz\"")).ok()
    }
}

pub fn gzip(data: &[u8]) -> Bytes {
    use std::io::Write;
    let mut enc = flate2::write::GzEncoder::new(
        Vec::with_capacity(data.len() / 2 + 32),
        flate2::Compression::new(6),
    );
    let _ = enc.write_all(data);
    Bytes::from(enc.finish().unwrap_or_default())
}

pub fn gunzip(data: &[u8]) -> Option<Bytes> {
    use std::io::Read;
    let mut out = Vec::with_capacity(data.len() * 3);
    flate2::read::GzDecoder::new(data)
        .take(8 << 20)
        .read_to_end(&mut out)
        .ok()?;
    Some(Bytes::from(out))
}

pub type IconCache = Cache<String, Arc<Icon>>;

fn icon_cache(cfg: &CacheCfg) -> IconCache {
    Cache::builder()
        .max_capacity(cfg.memory_max_bytes / 2)
        .weigher(|k: &String, v: &Arc<Icon>| (k.len() + v.size()).try_into().unwrap_or(u32::MAX))
        .time_to_live(Duration::from_secs(cfg.memory_ttl_secs))
        .build()
}

pub struct Store {
    backend: Option<Arc<dyn ObjectStore>>,
    seed: Option<Arc<dyn ObjectStore>>,
    pub disk: Option<Arc<Disk>>,
    attributes: bool,
    prefix: Path,
    timeout: Duration,
    object_cache_control: String,
    meta_mem: Cache<String, Arc<Meta>>,
    absent: Cache<String, ()>,
    img_color: IconCache,
    img_mono: IconCache,
    compress: bool,
    pub plated: IconCache,
}

impl Store {
    pub fn new(cfg: &CacheCfg, http: &HttpCache) -> anyhow::Result<Self> {
        let (backend, prefix, attributes, local) = build_backend(cfg)?;
        if let Some(root) = local.clone() {
            std::thread::spawn(move || drop_old_layouts(&root));
        }
        let disk = local.and_then(|root| {
            Disk::new(
                root.join(LAYOUT_VERSION),
                cfg.max_disk_bytes,
                cfg.max_disk_entries,
            )
            .map(Arc::new)
        });
        let seed = cfg
            .seed
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|dir| match object_store::local::LocalFileSystem::new_with_prefix(dir) {
                Ok(fs) => Some(Arc::new(fs) as Arc<dyn ObjectStore>),
                Err(e) => {
                    tracing::warn!(dir, error = %e, "cache.seed is not readable; running without it");
                    None
                }
            });
        Ok(Self {
            backend,
            seed,
            disk,
            attributes,
            prefix,
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
            compress: http.compression == crate::config::Compression::Gzip,
            plated: Cache::builder()
                .max_capacity(cfg.memory_max_bytes / 4)
                .weigher(|k: &String, v: &Arc<Icon>| {
                    (k.len() + v.size()).try_into().unwrap_or(u32::MAX)
                })
                .time_to_live(Duration::from_secs(cfg.memory_ttl_secs))
                .build(),
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

    fn seed_path(domain: &str, name: &str) -> Path {
        Path::from(format!("{LAYOUT_VERSION}/{domain}/{name}"))
    }

    async fn read_object(&self, domain: &str, name: &str) -> anyhow::Result<Option<Bytes>> {
        if let Some(backend) = &self.backend
            && let Some(b) = self.read(backend, &self.path(domain, name)).await?
        {
            return Ok(Some(b));
        }
        if let Some(seed) = &self.seed {
            return self.read(seed, &Self::seed_path(domain, name)).await;
        }
        Ok(None)
    }

    pub async fn peek_meta(&self, domain: &str) -> Option<Meta> {
        if let Some(m) = self.meta_mem.get(domain) {
            return Some((*m).clone());
        }
        let bytes = self.read_object(domain, "meta.json").await.ok()??;
        serde_json::from_slice::<Meta>(&bytes)
            .ok()
            .filter(|m| m.domain == domain)
    }

    pub async fn meta(&self, domain: &str) -> Option<Arc<Meta>> {
        if let Some(m) = self.meta_mem.get(domain) {
            return Some(m);
        }
        if self.backend.is_none() && self.seed.is_none() {
            return None;
        }
        if self.absent.contains_key(domain) {
            return None;
        }
        match self.read_object(domain, "meta.json").await {
            Ok(Some(bytes)) => match serde_json::from_slice::<Meta>(&bytes) {
                Ok(meta) if meta.domain == domain => {
                    if let Some(disk) = self.disk.clone() {
                        let domain = domain.to_string();
                        tokio::task::spawn_blocking(move || disk.touch(&domain));
                    }
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
        if self.backend.is_none() && self.seed.is_none() {
            return None;
        }
        for gzipped in [self.compress, !self.compress] {
            match self.read_object(domain, file_name(slot, gzipped)).await {
                Ok(Some(b)) => {
                    let body = match (gzipped, self.compress) {
                        (true, false) => gunzip(&b)?,
                        (false, true) => gzip(&b),
                        _ => b,
                    };
                    let icon = Icon::of(meta, slot, body, self.compress);
                    cache.insert(domain.to_string(), icon.clone());
                    return Some(icon);
                }
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(domain, error = %e, "storage read failed");
                    return None;
                }
            }
        }
        None
    }

    pub async fn put(&self, meta: Meta, images: Vec<(Slot, Vec<u8>)>) {
        self.write(meta, images, true).await;
    }

    pub async fn put_cold(&self, meta: Meta, images: Vec<(Slot, Vec<u8>)>) {
        self.write(meta, images, false).await;
    }

    async fn write(&self, meta: Meta, images: Vec<(Slot, Vec<u8>)>, warm: bool) {
        let domain = meta.domain.clone();
        let new_entry = self.disk.as_ref().is_some_and(|d| !d.exists(&domain));
        let mut sizes = Vec::with_capacity(3);
        for (slot, data) in images {
            let name = file_name(slot, self.compress);
            let bytes = if self.compress {
                gzip(&data)
            } else {
                Bytes::from(data)
            };
            sizes.push(bytes.len());
            if let Some(backend) = &self.backend {
                let mut opts = self.put_options(SVG_MIME, &self.object_cache_control);
                if self.compress && self.attributes {
                    opts.attributes
                        .insert(Attribute::ContentEncoding, "gzip".to_string().into());
                }
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
            if warm {
                self.icons(slot)
                    .insert(domain.clone(), Icon::of(&meta, slot, bytes, self.compress));
            } else {
                self.icons(slot).invalidate(&domain);
            }
        }
        if let Some(backend) = &self.backend {
            match serde_json::to_vec(&meta) {
                Ok(json) => {
                    sizes.push(json.len());
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
        if let Some(disk) = &self.disk {
            disk.added(sizes, new_entry);
        }
        self.absent.invalidate(&domain);
        if warm {
            self.meta_mem.insert(domain, Arc::new(meta));
        } else {
            self.meta_mem.invalidate(&domain);
        }
    }

    pub fn forget(&self, domains: &[String]) {
        for d in domains {
            self.meta_mem.invalidate(d);
            self.img_color.invalidate(d);
            self.img_mono.invalidate(d);
        }
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

type Backend = (Option<Arc<dyn ObjectStore>>, Path, bool, Option<PathBuf>);

fn build_backend(cfg: &CacheCfg) -> anyhow::Result<Backend> {
    let spec = cfg.storage.trim();
    if spec.is_empty() || spec.eq_ignore_ascii_case("none") || spec.eq_ignore_ascii_case("memory") {
        return Ok((None, Path::default(), false, None));
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
        return Ok((Some(Arc::new(fs)), Path::default(), false, Some(dir)));
    }
    let attributes = !matches!(url.scheme(), "memory");
    let (store, prefix) = object_store::parse_url_opts(&url, cfg.options.iter())?;
    Ok((Some(Arc::from(store)), prefix, attributes, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_old_cache_layouts_only() {
        let dir = std::env::temp_dir().join(format!("favicons-layouts-{}", std::process::id()));
        for d in ["v3/example.com", "v4/example.com", "vendor", "catalogs"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        super::drop_old_layouts(&dir);
        assert!(!dir.join("v3").exists());
        assert!(dir.join("v4/example.com").exists());
        assert!(dir.join("vendor").exists() && dir.join("catalogs").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

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
            fresh.image(&meta, Slot::Mono).await.unwrap().raw().as_ref(),
            [4, 5]
        );
        assert!(dir.join("v4/example.com/color.svg.gz").exists());
        assert!(dir.join("v4/example.com/mono.svg.gz").exists());
        assert!(!dir.join("v4/example.com/color.svg").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn reads_either_layout_and_keeps_one_on_disk() {
        let dir = std::env::temp_dir().join(format!("favicons-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(dir.join("v4/old.com")).unwrap();
        let mut meta = Meta::new("old.com", Status::Ok);
        meta.color = Some(VectorOrigin::Original);
        std::fs::write(
            dir.join("v4/old.com/meta.json"),
            serde_json::to_vec(&meta).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("v4/old.com/color.svg"), b"<svg/>").unwrap();

        let gz = store_in(&dir);
        let icon = gz.image(&meta, Slot::Color).await.unwrap();
        assert!(icon.gzipped);
        assert_eq!(icon.raw().as_ref(), b"<svg/>");
        assert_eq!(gunzip(icon.gzip().unwrap()).unwrap().as_ref(), b"<svg/>");

        let plain = Store::new(
            &CacheCfg {
                storage: format!("file://{}", dir.display()),
                ..CacheCfg::default()
            },
            &HttpCache {
                compression: crate::config::Compression::Off,
                ..HttpCache::default()
            },
        )
        .unwrap();
        plain
            .put(
                meta.clone(),
                vec![(Slot::Mono, b"<svg id=\"m\"/>".to_vec())],
            )
            .await;
        assert!(dir.join("v4/old.com/mono.svg").exists());
        assert!(!dir.join("v4/old.com/mono.svg.gz").exists());
        let mono = store_in(&dir).image(&meta, Slot::Mono).await.unwrap();
        assert_eq!(mono.raw().as_ref(), b"<svg id=\"m\"/>");
        let _ = std::fs::remove_dir_all(dir);
    }
}
