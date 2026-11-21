use std::sync::Arc;

use anyhow::Context;
use std::time::Duration;

use bytes::Bytes;
use moka::sync::Cache;
use tokio::sync::Semaphore;
use url::Url;

use crate::age::DomainAge;
use crate::config::{Config, SandboxMode};
use crate::contract::{MonoSource, Provenance, Slot, Style};
use crate::discover::{self, Candidate, Location, Page, Scope};
use crate::dns::{DnsError, SafeDns};
use crate::domain::normalize;
use crate::limiter::{ClientKeyer, RateLimiter};
use crate::metrics::{Stats, inc};
use crate::net::{FetchError, Fetched, HttpClient, Redirects};
use crate::palette::Palette;
use crate::policy::Policy;
use crate::queue::FairQueue;
use crate::sandbox::Renderer;
use crate::store::{Icon, Meta, Status, Store, now};
use crate::vector::Vector;

const ACCEPT_HTML: &str = "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1";
const ACCEPT_IMAGE: &str =
    "image/png,image/x-icon,image/vnd.microsoft.icon,image/svg+xml,image/webp,image/*;q=0.8";
const MAX_MONO_ATTEMPTS: usize = 2;

pub struct App {
    pub cfg: Config,
    pub store: Store,
    pub policy: Policy,
    pub catalog: Option<crate::catalog::Catalog>,
    pub slugs: crate::slugs::Catalogs,
    pub client: HttpClient,
    renderer: Renderer,
    dns: Arc<SafeDns>,
    list_client: HttpClient,
    pub age: Option<DomainAge>,
    pub queue: FairQueue,
    pub limiter: RateLimiter,
    pub keyer: ClientKeyer,
    pub stats: Stats,
    generated: Cache<([u8; 32], Style), Arc<Icon>>,
    pub profiles: crate::http::Profiles,
    pub challenges: Arc<crate::tls::Challenges>,
    pub site: crate::site::Site,
    generated_palettes: Cache<[u8; 32], Palette>,
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
            profiles: crate::http::Profiles::new(&cfg.http_cache)?,
            challenges: Arc::new(crate::tls::Challenges::default()),
            site: crate::site::Site::new(&cfg),
            generated_palettes: Cache::builder()
                .max_capacity(10_000)
                .time_to_idle(Duration::from_secs(3_600))
                .build(),
            policy: Policy::new(cfg.policy.clone()),
            catalog: crate::catalog::Catalog::new(&cfg.catalog.thesvg),
            slugs: crate::slugs::Catalogs::new(&cfg.catalogs),
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
                .weigher(|_: &([u8; 32], Style), v: &Arc<Icon>| {
                    (32 + v.data.len()).try_into().unwrap_or(u32::MAX)
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

    pub fn warning_icon(&self, style: Style) -> Arc<Icon> {
        self.generated_icon([0; 32], style, crate::vector::warning)
    }

    fn seed(&self, seed_source: &str) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.cfg.identicon.salt.as_bytes());
        hasher.update(b"\0");
        hasher.update(seed_source.as_bytes());
        *hasher.finalize().as_bytes()
    }

    pub fn identicon(&self, seed_source: &str, style: Style) -> Arc<Icon> {
        let seed = self.seed(seed_source);
        self.generated_icon(seed, style, |mono| crate::vector::identicon(&seed, mono))
    }

    pub async fn analyze(&self, svg: &[u8]) -> Option<Palette> {
        let svg = std::str::from_utf8(svg).ok()?;
        let _permit = self.decode_slots.clone().acquire_owned().await.ok()?;
        match self.renderer.analyze(&self.cfg.render, svg).await {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::debug!(error = %e, "palette analysis failed");
                None
            }
        }
    }

    fn generated_icon(
        &self,
        key: [u8; 32],
        style: Style,
        draw: impl Fn(bool) -> String,
    ) -> Arc<Icon> {
        self.generated.get_with((key, style), || {
            let palette = self.generated_palettes.get(&key).or_else(|| {
                let p = crate::palette::analyze_svg(&draw(false)).ok()?;
                self.generated_palettes.insert(key, p);
                Some(p)
            });
            let data = Bytes::from(draw(style == Style::Mono));
            let etag = format!("g-{}", &blake3::hash(&data).to_hex()[..20]);
            Icon::new(data, Some(&etag), None, palette)
        })
    }

    pub fn spawn_workers(self: &Arc<Self>) {
        for _ in 0..self.cfg.fetch.workers {
            let app = self.clone();
            tokio::spawn(async move {
                loop {
                    let domain = app.queue.pop().await;
                    if let Some(prev) = app.store.meta(&domain).await
                        && !app.is_stale(&prev)
                        && app.refresh_palette(&prev).await
                    {
                        app.queue.complete(&domain);
                        continue;
                    }
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
                    if meta.status == Status::Ok
                        && meta.palette.is_none()
                        && let Some((_, svg)) = images.iter().find(|(s, _)| *s == Slot::Color)
                    {
                        meta.palette = app.analyze(svg).await;
                        meta.palette_rev = crate::palette::REVISION;
                    }
                    tracing::info!(domain, status = ?meta.status, ms = started.elapsed().as_millis() as u64, "fetched");
                    app.store.put(meta, images).await;
                    app.queue.complete(&domain);
                }
            });
        }
    }

    async fn refresh_palette(&self, prev: &Meta) -> bool {
        if prev.status != Status::Ok
            || prev.color.is_none()
            || (prev.palette.is_some() && prev.palette_rev >= crate::palette::REVISION)
        {
            return false;
        }
        let Some(color) = self.store.image(prev, Slot::Color).await else {
            return false;
        };
        let Some(palette) = self.analyze(&color.data).await else {
            return false;
        };
        let mut images = vec![(Slot::Color, color.data.to_vec())];
        if prev.mono.is_some()
            && let Some(mono) = self.store.image(prev, Slot::Mono).await
        {
            images.push((Slot::Mono, mono.data.to_vec()));
        }
        let meta = Meta {
            palette: Some(palette),
            palette_rev: crate::palette::REVISION,
            ..prev.clone()
        };
        self.store.put(meta, images).await;
        true
    }

    async fn fetch_domain(&self, domain: &str) -> (Meta, Vec<(Slot, Vec<u8>)>) {
        if let Some((id, slug)) = crate::slugs::parse_key(domain) {
            return self.fetch_catalog_item(domain, id, slug).await;
        }
        if let Some(age) = &self.age
            && age.is_too_new(domain).await
        {
            return (Meta::new(domain, Status::TooNew), vec![]);
        }
        if let Err(DnsError::Blocked) = self.dns.resolve(domain).await {
            tracing::info!(domain, "blocked by the filtering DNS resolver");
            return (Meta::new(domain, Status::Blocked), vec![]);
        }
        if let Some(found) = self.try_curated(domain).await {
            return found;
        }
        let f = &self.cfg.fetch;
        let mut origin = None;
        let mut candidates = Vec::new();
        let mut reachable = false;
        let mut manifest_declared = false;

        let roots = [
            format!("https://{domain}/"),
            format!("https://www.{domain}/"),
            format!("http://{domain}/"),
        ];
        for root in roots {
            let Ok(url) = Url::parse(&root) else { continue };
            if !self.client.urls().allows(&url) {
                continue;
            }
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
        let mut images: Vec<(Slot, Vec<u8>)> = Vec::new();
        let mut any_transport_ok = reachable;
        for cand in discover::rank(candidates.clone())
            .into_iter()
            .take(f.max_candidates)
        {
            let label = match &cand.location {
                Location::Remote(u) => u.to_string(),
                Location::Inline(b) => format!("inline:{}b", b.len()),
            };
            let (vector, transport_ok) = self.try_candidate(domain, cand).await;
            any_transport_ok |= transport_ok;
            let Some(vector) = vector else { continue };
            tracing::debug!(
                domain,
                source = label,
                origin = vector.origin.as_str(),
                bytes = vector.svg.len(),
                "icon selected"
            );
            let etag = blake3::hash(vector.svg.as_bytes()).to_hex()[..20].to_string();
            meta.etags
                .insert(crate::store::object_name(Slot::Color).into(), etag);
            meta.color = Some(vector.origin);
            meta.provenance = Some(Provenance::Site);
            images.push((Slot::Color, vector.svg.into_bytes()));
            break;
        }
        if meta.color.is_none() {
            for template in &self.cfg.upstream.providers {
                let Some(url) = crate::config::Upstream::url(template, domain) else {
                    continue;
                };
                let host = url.host_str().unwrap_or_default().to_string();
                let cand = Candidate {
                    location: Location::Remote(url),
                    declared: 0,
                    svg: false,
                    theme: None,
                    mono: false,
                };
                let (vector, _) = self.try_candidate(domain, cand).await;
                let Some(vector) = vector else { continue };
                tracing::debug!(domain, upstream = host, "icon taken from upstream provider");
                let etag = blake3::hash(vector.svg.as_bytes()).to_hex()[..20].to_string();
                meta.etags
                    .insert(crate::store::object_name(Slot::Color).into(), etag);
                meta.color = Some(vector.origin);
                meta.upstream = Some(host);
                meta.provenance = Some(Provenance::Upstream);
                images.push((Slot::Color, vector.svg.into_bytes()));
                break;
            }
        }
        if let Some((source, vector)) = self.monochrome(domain, &candidates, &images).await {
            let etag = blake3::hash(vector.svg.as_bytes()).to_hex()[..20].to_string();
            meta.etags
                .insert(crate::store::object_name(Slot::Mono).into(), etag);
            meta.mono = Some(vector.origin);
            meta.mono_source = Some(source);
            images.push((Slot::Mono, vector.svg.into_bytes()));
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
            urls: self.client.urls().clone(),
            max_inline: self.cfg.fetch.max_icon_bytes,
        }
    }

    async fn host_allowed(&self, site: &str, url: &Url) -> Result<(), &'static str> {
        let host = url.host_str().ok_or("no host")?;
        if !matches!(url.host(), Some(url::Host::Domain(_))) {
            if !self.client.urls().allows(url) {
                return Err("ip hosts not allowed");
            }
            return if self.policy.is_blocked(host, host) {
                Err("blocklisted")
            } else {
                Ok(())
            };
        }
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
        color: &[(Slot, Vec<u8>)],
    ) -> Option<(MonoSource, Vector)> {
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
                Ok(v) => return Some((MonoSource::MaskIcon, v)),
                Err(e) => tracing::debug!(domain, error = %e, "mask icon rejected"),
            }
        }
        let source = color.iter().find(|(s, _)| *s == Slot::Color)?;
        match self.mono_job(MonoSource::Derived, source.1.clone()).await {
            Ok(v) => Some((MonoSource::Derived, v)),
            Err(e) => {
                tracing::debug!(domain, error = %e, "monochrome derivation failed");
                None
            }
        }
    }

    async fn mono_job(&self, source: MonoSource, bytes: Vec<u8>) -> anyhow::Result<Vector> {
        let _permit = self.decode_slots.clone().acquire_owned().await?;
        self.renderer.mono(&self.cfg.render, source, bytes).await
    }

    async fn try_candidate(&self, domain: &str, cand: Candidate) -> (Option<Vector>, bool) {
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
            Ok(v) => (Some(v), transport_ok),
            Err(e) => {
                inc(&self.stats.decode_errors);
                tracing::debug!(domain, error = %e, "icon rejected");
                (None, transport_ok)
            }
        }
    }

    pub fn spawn_catalog_reloader(self: &Arc<Self>) {
        if self.catalog.is_none() {
            return;
        }
        let app = self.clone();
        tokio::spawn(async move {
            let every = Duration::from_secs(app.cfg.catalog.thesvg.reload_secs.max(3_600));
            loop {
                if let Some(catalog) = &app.catalog {
                    catalog.reload(&app.list_client).await;
                }
                tokio::time::sleep(every).await;
            }
        });
    }

    async fn bimi_logo(&self, domain: &str) -> Option<Url> {
        if !self.cfg.bimi.enabled {
            return None;
        }
        let records = self
            .dns
            .txt(&format!("default._bimi.{domain}"))
            .await
            .ok()?;
        crate::dns::bimi_location(&records)
    }

    async fn try_curated(&self, domain: &str) -> Option<(Meta, Vec<(Slot, Vec<u8>)>)> {
        let mut sources: Vec<(Url, Provenance, Option<String>, Option<Url>)> = Vec::new();
        if let Some(url) = self.bimi_logo(domain).await {
            sources.push((url, Provenance::Bimi, None, None));
        }
        if let Some(entry) = self.catalog.as_ref().and_then(|c| c.lookup(domain)) {
            sources.push((
                entry.color,
                Provenance::Thesvg,
                Some(entry.license),
                entry.mono,
            ));
        }
        for (url, provenance, license, mono_url) in sources {
            if let Some(found) = self
                .curated(
                    domain,
                    domain,
                    Location::Remote(url),
                    true,
                    provenance,
                    license,
                    mono_url,
                )
                .await
            {
                return Some(found);
            }
        }
        None
    }

    async fn fetch_catalog_item(
        &self,
        key: &str,
        id: &str,
        slug: &str,
    ) -> (Meta, Vec<(Slot, Vec<u8>)>) {
        let Some(entry) = self.slugs.get(id).and_then(|c| c.lookup(slug)) else {
            return (Meta::new(key, Status::NotFound), vec![]);
        };
        let p = &self.cfg.policy;
        let site = entry
            .color
            .host_str()
            .and_then(|h| normalize(h, p.collapse_private_suffixes, &p.deny_suffixes))
            .map(|n| n.registrable)
            .unwrap_or_default();
        let svg = entry.color.path().ends_with(".svg");
        let location = match &entry.tint {
            Some(hex) => {
                let max = self.cfg.fetch.max_icon_bytes;
                let Ok(r) = self
                    .get_checked(&site, &entry.color, ACCEPT_IMAGE, max, false)
                    .await
                else {
                    return (Meta::new(key, Status::FetchFailed), vec![]);
                };
                match crate::slugs::tint_svg(&r.bytes, hex) {
                    Some(tinted) => Location::Inline(tinted),
                    None => return (Meta::new(key, Status::FetchFailed), vec![]),
                }
            }
            None => Location::Remote(entry.color.clone()),
        };
        match self
            .curated(
                key,
                &site,
                location,
                svg,
                Provenance::Catalog,
                Some(entry.license),
                entry.mono,
            )
            .await
        {
            Some(found) => found,
            None => (Meta::new(key, Status::FetchFailed), vec![]),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn curated(
        &self,
        domain: &str,
        site: &str,
        location: Location,
        svg: bool,
        provenance: Provenance,
        license: Option<String>,
        mono_url: Option<Url>,
    ) -> Option<(Meta, Vec<(Slot, Vec<u8>)>)> {
        {
            let source = match &location {
                Location::Remote(url) => url.to_string(),
                Location::Inline(_) => "inline".to_string(),
            };
            let cand = Candidate {
                location,
                declared: discover::SIZE_ANY,
                svg,
                theme: None,
                mono: false,
            };
            let (vector, _) = self.try_candidate(site, cand).await;
            let vector = vector?;
            tracing::debug!(
                domain,
                source,
                provenance = provenance.as_str(),
                "curated icon selected"
            );
            let mut meta = Meta::new(domain, Status::Ok);
            meta.provenance = Some(provenance);
            meta.license = license;
            let etag = blake3::hash(vector.svg.as_bytes()).to_hex()[..20].to_string();
            meta.etags
                .insert(crate::store::object_name(Slot::Color).into(), etag);
            meta.color = Some(vector.origin);
            let colour_svg = vector.svg.into_bytes();
            let mut mono = None;
            if let Some(mono_url) = mono_url
                && let Ok(r) = self
                    .get_checked(
                        site,
                        &mono_url,
                        ACCEPT_IMAGE,
                        self.cfg.fetch.max_icon_bytes,
                        false,
                    )
                    .await
                && let Ok(v) = self.mono_job(MonoSource::MaskIcon, r.bytes).await
            {
                mono = Some((MonoSource::MaskIcon, v));
            }
            if mono.is_none()
                && let Ok(v) = self.mono_job(MonoSource::Derived, colour_svg.clone()).await
            {
                mono = Some((MonoSource::Derived, v));
            }
            let mut images = vec![(Slot::Color, colour_svg)];
            if let Some((source, v)) = mono {
                let etag = blake3::hash(v.svg.as_bytes()).to_hex()[..20].to_string();
                meta.etags
                    .insert(crate::store::object_name(Slot::Mono).into(), etag);
                meta.mono = Some(v.origin);
                meta.mono_source = Some(source);
                images.push((Slot::Mono, v.svg.into_bytes()));
            }
            Some((meta, images))
        }
    }

    pub fn spawn_disk_sweeper(self: &Arc<Self>) {
        let Some(disk) = self.store.disk.clone() else {
            return;
        };
        let app = self.clone();
        tokio::spawn(async move {
            loop {
                let d = disk.clone();
                let Ok(sweep) = tokio::task::spawn_blocking(move || d.sweep()).await else {
                    break;
                };
                app.store.forget(&sweep.evicted);
                if sweep.evicted.is_empty() {
                    tracing::debug!(
                        bytes = sweep.bytes,
                        entries = sweep.entries,
                        "disk cache measured"
                    );
                } else {
                    tracing::info!(
                        evicted = sweep.evicted.len(),
                        bytes = sweep.bytes,
                        entries = sweep.entries,
                        "disk cache trimmed"
                    );
                }
                tokio::time::sleep(Duration::from_secs(60)).await;
                if !disk.needs_sweep() {
                    let _ = tokio::time::timeout(Duration::from_secs(3_600), disk.wake.notified())
                        .await;
                }
            }
        });
    }

    pub fn spawn_slug_reloaders(self: &Arc<Self>) {
        for catalog in self.slugs.all() {
            let app = self.clone();
            let catalog = catalog.clone();
            tokio::spawn(async move {
                let every = Duration::from_secs(catalog.reload_secs().max(3_600));
                loop {
                    catalog.reload(&app.list_client).await;
                    let retry = if catalog.is_loaded() {
                        every
                    } else {
                        Duration::from_secs(300)
                    };
                    tokio::time::sleep(retry).await;
                }
            });
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
