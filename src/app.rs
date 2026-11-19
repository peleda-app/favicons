use std::sync::Arc;

use anyhow::Context;
use std::time::Duration;

use bytes::Bytes;
use moka::sync::Cache;
use tokio::sync::Semaphore;
use url::Url;

use crate::age::DomainAge;
use crate::config::{Config, SandboxMode};
use crate::contract::{MonoSource, Slot, Style, Theme};
use crate::discover::{self, Candidate, Location, Page, Scope};
use crate::dns::{DnsError, SafeDns};
use crate::domain::normalize;
use crate::limiter::{ClientKeyer, RateLimiter};
use crate::metrics::{Stats, inc};
use crate::net::{FetchError, Fetched, HttpClient, Redirects};
use crate::policy::Policy;
use crate::queue::FairQueue;
use crate::render::{self, Variant};
use crate::sandbox::Renderer;
use crate::store::{Meta, Status, Store, now};

const ACCEPT_HTML: &str = "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1";
const ACCEPT_IMAGE: &str =
    "image/png,image/x-icon,image/vnd.microsoft.icon,image/svg+xml,image/webp,image/*;q=0.8";
const MAX_DARK_ATTEMPTS: usize = 3;
const MAX_MONO_ATTEMPTS: usize = 2;

pub struct App {
    pub cfg: Config,
    pub store: Store,
    pub policy: Policy,
    pub client: HttpClient,
    renderer: Renderer,
    dns: Arc<SafeDns>,
    list_client: HttpClient,
    pub age: Option<DomainAge>,
    pub queue: FairQueue,
    pub limiter: RateLimiter,
    pub keyer: ClientKeyer,
    pub stats: Stats,
    pub generated: Cache<(String, u32, Theme, Style), Bytes>,
    decode_slots: Arc<Semaphore>,
    pub request_slots: Arc<Semaphore>,
}

impl App {
    pub fn new(cfg: Config) -> anyhow::Result<Arc<Self>> {
        let dns = SafeDns::new(&cfg.dns, cfg.fetch.dns_timeout())?;
        let client = HttpClient::new(&cfg.fetch, dns.clone(), Redirects::Manual)?;
        let list_client = HttpClient::new(
            &crate::config::Fetch {
                request_timeout_ms: 120_000,
                ..cfg.fetch.clone()
            },
            dns.clone(),
            Redirects::Follow,
        )?;
        Ok(Arc::new(Self {
            store: Store::new(&cfg.cache, &cfg.http_cache)?,
            policy: Policy::new(cfg.policy.clone()),
            renderer: match cfg.render.sandbox {
                SandboxMode::Off => Renderer::InProcess,
                SandboxMode::Process => Renderer::Process {
                    exe: Arc::new(std::env::current_exe().context("locating own executable")?),
                },
                SandboxMode::Socket => Renderer::Socket {
                    path: Arc::new(cfg.render.socket_path.clone()),
                },
            },
            age: DomainAge::new(list_client.clone(), &cfg.policy),
            dns,
            queue: FairQueue::new(
                cfg.fetch.queue_capacity,
                cfg.fetch.per_client_queue_capacity,
            ),
            limiter: RateLimiter::new(&cfg.rate_limit),
            keyer: ClientKeyer::new(cfg.client_ip.clone()),
            stats: Stats::default(),
            generated: Cache::builder()
                .max_capacity(16 * 1024 * 1024)
                .weigher(|k: &(String, u32, Theme, Style), v: &Bytes| {
                    (k.0.len() + v.len()).try_into().unwrap_or(u32::MAX)
                })
                .time_to_idle(Duration::from_secs(3_600))
                .build(),
            decode_slots: Arc::new(Semaphore::new(cfg.render.decode_concurrency)),
            request_slots: Arc::new(Semaphore::new(cfg.server.max_concurrent_requests)),
            client,
            list_client,
            cfg,
        }))
    }

    pub fn ttl_for(&self, status: Status) -> u64 {
        let c = &self.cfg.cache;
        match status {
            Status::Ok => c.ttl_secs,
            Status::NotFound => c.not_found_ttl_secs,
            Status::FetchFailed => c.fetch_failed_ttl_secs,
            Status::TooNew => c.too_new_ttl_secs,
            Status::Blocked => c.blocked_ttl_secs,
        }
    }

    pub fn is_stale(&self, meta: &Meta) -> bool {
        now().saturating_sub(meta.fetched_at) >= self.ttl_for(meta.status)
    }

    pub fn warning_icon(&self, size: u32, theme: Theme, style: Style) -> Bytes {
        self.generated_icon("\0blocked", size, theme, style, |mono, format| {
            render::warning_icon(size, theme, mono, format)
        })
    }

    pub fn identicon(&self, seed_source: &str, size: u32, theme: Theme, style: Style) -> Bytes {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.cfg.identicon.salt.as_bytes());
        hasher.update(b"\0");
        hasher.update(seed_source.as_bytes());
        let seed = *hasher.finalize().as_bytes();
        self.generated_icon(seed_source, size, theme, style, |mono, format| {
            render::identicon(&seed, size, theme, mono, format)
        })
    }

    fn generated_icon(
        &self,
        key: &str,
        size: u32,
        theme: Theme,
        style: Style,
        draw: impl FnOnce(bool, crate::contract::OutputFormat) -> anyhow::Result<Vec<u8>>,
    ) -> Bytes {
        let theme = if style == Style::Mono {
            Theme::Light
        } else {
            theme
        };
        let key = (key.to_string(), size, theme, style);
        if let Some(b) = self.generated.get(&key) {
            return b;
        }
        let data =
            Bytes::from(draw(style == Style::Mono, self.cfg.render.format).unwrap_or_default());
        self.generated.insert(key, data.clone());
        data
    }

    pub fn spawn_workers(self: &Arc<Self>) {
        for _ in 0..self.cfg.fetch.workers {
            let app = self.clone();
            tokio::spawn(async move {
                loop {
                    let domain = app.queue.pop().await;
                    let started = std::time::Instant::now();
                    let result = tokio::time::timeout(
                        app.cfg.fetch.job_timeout(),
                        app.fetch_domain(&domain),
                    )
                    .await;
                    let (mut meta, images) = match result {
                        Ok(r) => r,
                        Err(_) => (Meta::new(&domain, Status::FetchFailed), vec![]),
                    };
                    if matches!(meta.status, Status::NotFound | Status::FetchFailed)
                        && let Some(prev) = app.store.meta(&domain).await
                        && prev.status == Status::Ok
                    {
                        let c = &app.cfg.cache;
                        meta = Meta {
                            fetched_at: now()
                                .saturating_sub(c.ttl_secs)
                                .saturating_add(c.fetch_failed_ttl_secs),
                            ..(*prev).clone()
                        };
                    }
                    match meta.status {
                        Status::Ok => inc(&app.stats.fetch_ok),
                        Status::NotFound => inc(&app.stats.fetch_not_found),
                        Status::FetchFailed => inc(&app.stats.fetch_failed),
                        Status::TooNew => inc(&app.stats.fetch_too_new),
                        Status::Blocked => inc(&app.stats.fetch_blocked),
                    }
                    tracing::info!(domain, status = ?meta.status, ms = started.elapsed().as_millis() as u64, "fetched");
                    app.store.put(meta, images).await;
                    app.queue.complete(&domain);
                }
            });
        }
    }

    async fn fetch_domain(&self, domain: &str) -> (Meta, Vec<(Slot, u32, Vec<u8>)>) {
        if let Some(age) = &self.age
            && age.is_too_new(domain).await
        {
            return (Meta::new(domain, Status::TooNew), vec![]);
        }
        if let Err(DnsError::Blocked) = self.dns.resolve(domain).await {
            tracing::info!(domain, "blocked by the filtering DNS resolver");
            return (Meta::new(domain, Status::Blocked), vec![]);
        }
        let f = &self.cfg.fetch;
        let mut origin = None;
        let mut candidates = Vec::new();
        let mut reachable = false;
        let mut manifest_declared = false;

        let mut roots = vec![
            format!("https://{domain}/"),
            format!("https://www.{domain}/"),
        ];
        if f.allow_http {
            roots.push(format!("http://{domain}/"));
        }
        for root in roots {
            let Ok(url) = Url::parse(&root) else { continue };
            match self
                .get_checked(domain, &url, ACCEPT_HTML, f.max_html_bytes, true)
                .await
            {
                Ok(page) => {
                    reachable = true;
                    let base = page.final_url.clone();
                    if let Some(p) = self.parse_page(&page.final_url, page.bytes).await {
                        candidates.extend(p.icons);
                        if let Some(m) = p.manifest {
                            manifest_declared = true;
                            if !discover::has_large(&candidates) {
                                candidates.extend(self.fetch_manifest(domain, &m).await);
                            }
                        }
                    }
                    origin = Some(base);
                    break;
                }
                Err(
                    FetchError::Status(_) | FetchError::Forbidden | FetchError::TooManyRedirects,
                ) => {
                    reachable = true;
                    origin.get_or_insert(url);
                }
                Err(e) => tracing::debug!(domain, root, error = %e, "root fetch failed"),
            }
        }
        let origin =
            origin.unwrap_or_else(|| Url::parse(&format!("https://{domain}/")).expect("valid url"));
        if reachable && !manifest_declared && !discover::has_large(&candidates) {
            for url in discover::guessed_manifests(&origin) {
                let found = self.fetch_manifest(domain, &url).await;
                if !found.is_empty() {
                    tracing::debug!(domain, url = %url, "guessed manifest found");
                    candidates.extend(found);
                    break;
                }
            }
        }
        candidates.extend(discover::default_candidates(&origin));
        tracing::debug!(
            domain,
            total = candidates.len(),
            mono = candidates.iter().filter(|c| c.mono).count(),
            "candidates discovered"
        );

        let mut meta = Meta::new(domain, Status::NotFound);
        meta.format = self.cfg.render.format;
        let mut images = Vec::new();
        let mut any_transport_ok = reachable;
        let max_size = self.cfg.render.sizes.iter().copied().max().unwrap_or(128);
        for theme in [Theme::Light, Theme::Dark] {
            let ranked = discover::rank(candidates.clone(), theme);
            let limit = if theme == Theme::Light {
                f.max_candidates
            } else {
                MAX_DARK_ATTEMPTS
            };
            let mut best: Option<(String, Vec<Variant>)> = None;
            for cand in ranked.into_iter().take(limit) {
                let label = match &cand.location {
                    Location::Remote(u) => u.to_string(),
                    Location::Inline(b) => format!("inline:{}b", b.len()),
                };
                let (variants, transport_ok) = self.try_candidate(domain, cand).await;
                any_transport_ok |= transport_ok;
                let Some(variants) = variants else { continue };
                let largest = |v: &[Variant]| v.iter().map(|x| x.size).max().unwrap_or(0);
                if best
                    .as_ref()
                    .is_none_or(|(_, b)| largest(&variants) > largest(b))
                {
                    best = Some((label, variants));
                }
                if best.as_ref().is_some_and(|(_, b)| largest(b) >= max_size) {
                    break;
                }
            }
            let Some((label, variants)) = best else {
                continue;
            };
            let sizes: Vec<u32> = variants.iter().map(|v| v.size).collect();
            tracing::debug!(
                domain,
                theme = theme.as_str(),
                source = label,
                ?sizes,
                "icon selected"
            );
            for v in variants {
                let etag = blake3::hash(&v.data).to_hex()[..20].to_string();
                let name = crate::store::object_name(theme, v.size, meta.format);
                meta.etags.insert(name, etag);
                images.push((theme.into(), v.size, v.data));
            }
            match theme {
                Theme::Light => meta.light = sizes,
                Theme::Dark => meta.dark = sizes,
            }
        }
        if let Some((source, variants)) = self.monochrome(domain, &candidates, &images).await {
            meta.mono = variants.iter().map(|v| v.size).collect();
            meta.mono_source = Some(source);
            for v in variants {
                let etag = blake3::hash(&v.data).to_hex()[..20].to_string();
                let name = crate::store::object_name(Slot::Mono, v.size, meta.format);
                meta.etags.insert(name, etag);
                images.push((Slot::Mono, v.size, v.data));
            }
        }
        meta.status = if !images.is_empty() {
            Status::Ok
        } else if any_transport_ok {
            Status::NotFound
        } else {
            Status::FetchFailed
        };
        (meta, images)
    }

    fn scope(&self) -> Scope {
        Scope {
            allow_http: self.client.allow_http(),
            max_inline: self.cfg.fetch.max_icon_bytes,
        }
    }

    async fn host_allowed(&self, site: &str, url: &Url) -> Result<(), &'static str> {
        let host = url.host_str().ok_or("no host")?;
        let p = &self.cfg.policy;
        let n = normalize(host, p.collapse_private_suffixes, &p.deny_suffixes)
            .ok_or("not a public domain")?;
        if self.policy.is_blocked(&n.host, &n.registrable) {
            return Err("blocklisted");
        }
        if n.registrable == site {
            return Ok(());
        }
        if let Some(age) = &self.age
            && age.is_too_new(&n.registrable).await
        {
            return Err("domain too new");
        }
        Ok(())
    }

    async fn get_checked(
        &self,
        site: &str,
        url: &Url,
        accept: &str,
        max: usize,
        head_only: bool,
    ) -> Result<Fetched, FetchError> {
        let mut current = url.clone();
        for _ in 0..=self.cfg.fetch.max_redirects {
            if let Err(reason) = self.host_allowed(site, &current).await {
                tracing::debug!(site, url = %current, reason, "host rejected");
                return Err(FetchError::Forbidden);
            }
            let res = if head_only {
                self.client.get_head(&current, accept, max).await
            } else {
                self.client.get(&current, accept, max).await
            };
            match res {
                Err(FetchError::Redirect(next)) => current = next,
                other => return other,
            }
        }
        Err(FetchError::TooManyRedirects)
    }

    async fn parse_page(&self, url: &Url, html: Vec<u8>) -> Option<Page> {
        let _permit = self.decode_slots.clone().acquire_owned().await.ok()?;
        match self
            .renderer
            .parse_html(&self.cfg.render, url, &self.scope(), html)
            .await
        {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::debug!(url = %url, error = %e, "html rejected");
                None
            }
        }
    }

    async fn fetch_manifest(&self, site: &str, url: &Url) -> Vec<Candidate> {
        let accept = "application/manifest+json, application/json;q=0.9";
        let max = self.cfg.fetch.max_manifest_bytes;
        let Ok(resp) = self.get_checked(site, url, accept, max, false).await else {
            return vec![];
        };
        let Ok(_permit) = self.decode_slots.clone().acquire_owned().await else {
            return vec![];
        };
        self.renderer
            .parse_manifest(&self.cfg.render, &resp.final_url, &self.scope(), resp.bytes)
            .await
            .unwrap_or_default()
    }

    async fn monochrome(
        &self,
        domain: &str,
        candidates: &[Candidate],
        color: &[(Slot, u32, Vec<u8>)],
    ) -> Option<(MonoSource, Vec<Variant>)> {
        for cand in discover::rank_mono(candidates)
            .into_iter()
            .take(MAX_MONO_ATTEMPTS)
        {
            let bytes = match cand.location {
                Location::Inline(b) => b,
                Location::Remote(url) => {
                    let max = self.cfg.fetch.max_icon_bytes;
                    match self
                        .get_checked(domain, &url, ACCEPT_IMAGE, max, false)
                        .await
                    {
                        Ok(r) => r.bytes,
                        Err(e) => {
                            tracing::debug!(domain, url = %url, error = %e, "mask icon fetch failed");
                            continue;
                        }
                    }
                }
            };
            match self.mono_job(MonoSource::MaskIcon, bytes).await {
                Ok(v) if !v.is_empty() => return Some((MonoSource::MaskIcon, v)),
                Ok(_) => {}
                Err(e) => tracing::debug!(domain, error = %e, "mask icon rejected"),
            }
        }
        let source = [Slot::Light, Slot::Dark].iter().find_map(|slot| {
            color
                .iter()
                .filter(|(s, ..)| s == slot)
                .max_by_key(|(_, size, _)| *size)
        })?;
        match self.mono_job(MonoSource::Derived, source.2.clone()).await {
            Ok(v) if !v.is_empty() => Some((MonoSource::Derived, v)),
            Ok(_) => None,
            Err(e) => {
                tracing::debug!(domain, error = %e, "monochrome derivation failed");
                None
            }
        }
    }

    async fn mono_job(&self, source: MonoSource, bytes: Vec<u8>) -> anyhow::Result<Vec<Variant>> {
        let _permit = self.decode_slots.clone().acquire_owned().await?;
        self.renderer.mono(&self.cfg.render, source, bytes).await
    }

    async fn try_candidate(&self, domain: &str, cand: Candidate) -> (Option<Vec<Variant>>, bool) {
        let (bytes, transport_ok) = match cand.location {
            Location::Inline(b) => (b, true),
            Location::Remote(url) => match self
                .get_checked(
                    domain,
                    &url,
                    ACCEPT_IMAGE,
                    self.cfg.fetch.max_icon_bytes,
                    false,
                )
                .await
            {
                Ok(r) => (r.bytes, true),
                Err(e) => {
                    tracing::debug!(domain, url = %url, error = %e, "icon fetch failed");
                    return (
                        None,
                        matches!(
                            e,
                            FetchError::Status(_) | FetchError::TooLarge(_) | FetchError::Forbidden
                        ),
                    );
                }
            },
        };
        let Ok(_permit) = self.decode_slots.clone().acquire_owned().await else {
            return (None, transport_ok);
        };
        match self.renderer.render(&self.cfg.render, bytes).await {
            Ok(v) if !v.is_empty() => (Some(v), transport_ok),
            Ok(_) => (None, transport_ok),
            Err(e) => {
                inc(&self.stats.decode_errors);
                tracing::debug!(domain, error = %e, "icon rejected");
                (None, transport_ok)
            }
        }
    }

    pub fn spawn_policy_reloader(self: &Arc<Self>) {
        let app = self.clone();
        tokio::spawn(async move {
            let every = Duration::from_secs(app.cfg.policy.reload_secs.max(60));
            loop {
                app.policy.reload(&app.list_client).await;
                tokio::time::sleep(every).await;
            }
        });
    }
}
