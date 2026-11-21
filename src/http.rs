use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use http::header::{
    ACCEPT_ENCODING, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, ACCESS_CONTROL_MAX_AGE,
    CACHE_CONTROL, CONTENT_ENCODING, CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, IF_NONE_MATCH,
    LOCATION, REFERRER_POLICY, RETRY_AFTER, VARY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};

use crate::app::App;
use crate::config::{CacheProfile, HttpCache};
use crate::contract::{
    FallbackReason, H_BLOCKED, H_CDN_CACHE_CONTROL, H_DOMAIN, H_FALLBACK, H_FETCH, H_LICENSE,
    H_MONO_SOURCE, H_PROVENANCE, H_REASON, H_SOURCE, H_STALE, H_STYLE, H_UPSTREAM, H_VECTOR,
    MonoSource, Provenance, SVG_MIME, Slot, Source, Style,
};
use crate::domain::{MAX_INPUT_LEN, normalize};
use crate::metrics::inc;
use crate::plate::Plate;
use crate::queue::{ClientKey, Enqueue};
use crate::site::Asset;
use crate::store::{Icon, Meta, Status};

pub fn router(app: Arc<App>) -> Router {
    let mut r = Router::new()
        .route("/robots.txt", page(Asset::Robots))
        .route("/llms.txt", page(Asset::Llms))
        .route("/openapi.json", page(Asset::OpenApi));
    if crate::site::ENABLED && app.cfg.server.landing {
        r = r
            .route("/", page(Asset::Index))
            .route("/sitemap.xml", page(Asset::Sitemap))
            .route("/config.example.toml", page(Asset::ConfigExample))
            .route("/docker-compose.yml", page(Asset::Compose))
            .route("/favicon.svg", page(Asset::Favicon))
            .route("/apple-touch-icon.png", page(Asset::AppleTouchIcon))
            .route("/og.png", page(Asset::OgImage))
            .route("/fonts/archivo.woff2", page(Asset::FontDisplay))
            .route("/fonts/martian-mono.woff2", page(Asset::FontMono));
    } else {
        r = r.route("/", page(Asset::OpenApi));
    }
    r.route("/{style}/{domain}", get(icon))
        .route("/{style}/{domain}/", get(icon_slash))
        .route("/{style}/{domain}/{slug}", get(catalog_icon))
        .route("/{style}/{domain}/{slug}/{theme}", get(plated_icon))
        .route(
            "/{style}/{domain}/{slug}/{theme}/{extra}",
            get(plated_catalog_icon),
        )
        .route("/catalogs.json", get(catalogs_index))
        .route("/stats", get(stats))
        .route("/catalogs/{file}", get(catalog_listing))
        .route("/.well-known/acme-challenge/{token}", get(acme_challenge))
        .route("/healthz", get(|| async { "ok" }))
        .fallback(|| async { plain(StatusCode::NOT_FOUND, "not found") })
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .layer(middleware::from_fn(preflight))
        .layer(middleware::map_response(security_headers))
        .with_state(app)
}

fn page(asset: Asset) -> axum::routing::MethodRouter<Arc<App>> {
    get(
        move |State(app): State<Arc<App>>, uri: Uri, h: HeaderMap| async move {
            app.site.serve(asset, &h, &uri)
        },
    )
}

async fn preflight(req: Request, next: Next) -> Response {
    if req.method() != http::Method::OPTIONS {
        return next.run(req).await;
    }
    let mut r = Response::new(Body::empty());
    *r.status_mut() = StatusCode::NO_CONTENT;
    let h = r.headers_mut();
    h.insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, HEAD, OPTIONS"),
    );
    h.insert(ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("*"));
    h.insert(ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("86400"));
    h.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    r
}

pub fn challenge_router(app: Arc<App>) -> Router {
    Router::new()
        .route("/.well-known/acme-challenge/{token}", get(acme_challenge))
        .fallback(to_https)
        .layer(middleware::map_response(security_headers))
        .with_state(app)
}

async fn acme_challenge(State(app): State<Arc<App>>, Path(token): Path<String>) -> Response {
    match app.challenges.get(&token) {
        Some(key_authorization) => {
            let mut r = Response::new(Body::from(key_authorization));
            r.headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
            r.headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            r
        }
        None => plain(StatusCode::NOT_FOUND, "not found"),
    }
}

async fn to_https(State(app): State<Arc<App>>, uri: Uri) -> Response {
    let Some(host) = app.cfg.tls.domains.first() else {
        return plain(StatusCode::NOT_FOUND, "not found");
    };
    let path = uri.path_and_query().map_or("/", |p| p.as_str());
    match HeaderValue::from_str(&format!("https://{host}{path}")) {
        Ok(loc) => {
            let mut r = Response::new(Body::empty());
            *r.status_mut() = StatusCode::PERMANENT_REDIRECT;
            r.headers_mut().insert(LOCATION, loc);
            r.headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            r
        }
        Err(_) => plain(StatusCode::BAD_REQUEST, "bad request"),
    }
}

pub fn admin_router(app: Arc<App>) -> Router {
    Router::new()
        .route(
            "/metrics",
            get(|State(app): State<Arc<App>>| async move {
                let blocked = app.policy.blocked_count();
                app.stats.render(
                    app.queue.len(),
                    app.queue.running(),
                    blocked,
                    app.catalog.as_ref().map_or(0, |c| c.len()),
                )
            }),
        )
        .route("/healthz", get(|| async { "ok" }))
        .with_state(app)
}

async fn guard(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    let Ok(_permit) = app.request_slots.clone().try_acquire_owned() else {
        inc(&app.stats.shed);
        let mut r = plain(StatusCode::SERVICE_UNAVAILABLE, "overloaded");
        r.headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from_static("1"));
        return r;
    };
    let timeout = Duration::from_millis(app.cfg.server.request_timeout_ms);
    let start = Instant::now();
    let mut r = match tokio::time::timeout(timeout, next.run(req)).await {
        Ok(r) => r,
        Err(_) => plain(StatusCode::SERVICE_UNAVAILABLE, "timeout"),
    };
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    if let Ok(v) = HeaderValue::from_str(&format!("app;dur={ms:.3}")) {
        r.headers_mut().insert(SERVER_TIMING, v);
    }
    r
}

const SERVER_TIMING: HeaderName = HeaderName::from_static("server-timing");
const TIMING_ALLOW_ORIGIN: HeaderName = HeaderName::from_static("timing-allow-origin");

const EXPOSED: &str = "etag, server-timing, cdn-cache-control, age, retry-after, x-icon-source, x-icon-domain, x-icon-style, x-icon-vector, \
     x-icon-provenance, x-icon-license, x-icon-upstream, x-icon-mono-source, x-icon-fallback, \
     x-icon-fallback-reason, x-icon-stale, x-icon-blocked, x-icon-shape, x-icon-fill, x-icon-accent, \
     x-icon-light-plate, x-icon-light-text, x-icon-light-mono, x-icon-light-mono-plate, \
     x-icon-dark-plate, x-icon-dark-text, x-icon-dark-mono, x-icon-dark-mono-plate";

async fn security_headers(mut res: Response) -> Response {
    let h = res.headers_mut();
    h.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.entry(CONTENT_SECURITY_POLICY)
        .or_insert(HeaderValue::from_static(
            "default-src 'none'; style-src 'unsafe-inline'; script-src 'none'; sandbox",
        ));
    h.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("cross-origin"),
    );
    h.insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert(TIMING_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert(
        ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static(EXPOSED),
    );
    h.entry(HeaderName::from_static("x-robots-tag"))
        .or_insert(HeaderValue::from_static("noindex"));
    res
}

fn plain(status: StatusCode, msg: &'static str) -> Response {
    let mut r = (status, msg).into_response();
    r.headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

pub struct CacheHeaders {
    browser: HeaderValue,
    cdn: HeaderValue,
}

impl CacheHeaders {
    fn new(name: &str, p: &CacheProfile) -> anyhow::Result<Self> {
        let parse = |v: &str| {
            HeaderValue::from_str(v)
                .map_err(|_| anyhow::anyhow!("http_cache.{name} has an invalid header value"))
        };
        Ok(Self {
            browser: parse(&p.browser)?,
            cdn: parse(&p.cdn)?,
        })
    }
}

pub struct Profiles {
    icon: CacheHeaders,
    canonical: CacheHeaders,
    fallback_transient: CacheHeaders,
    fallback_stable: CacheHeaders,
}

impl Profiles {
    pub fn new(c: &HttpCache) -> anyhow::Result<Self> {
        Ok(Self {
            icon: CacheHeaders::new("icon", &c.icon)?,
            canonical: CacheHeaders::new("canonical", &c.canonical)?,
            fallback_transient: CacheHeaders::new("fallback_transient", &c.fallback_transient)?,
            fallback_stable: CacheHeaders::new("fallback_stable", &c.fallback_stable)?,
        })
    }
}

#[derive(Clone)]
struct Req {
    name: String,
    valid: bool,
    style: Style,
    plate: Option<Plate>,
    if_none_match: Option<HeaderValue>,
    nested: bool,
    gzip: bool,
}

async fn icon(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
    Path((style, raw)): Path<(String, String)>,
) -> Response {
    serve_icon(app, peer, headers, uri, style, raw, false, None).await
}

async fn icon_slash(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
    Path((style, raw)): Path<(String, String)>,
) -> Response {
    serve_icon(app, peer, headers, uri, style, raw, true, None).await
}

async fn plated_icon(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
    Path((style, raw, shape, theme)): Path<(String, String, String, String)>,
) -> Response {
    if app.slugs.get(&raw).is_some()
        && let Some(plate) = Plate::auto(&theme)
    {
        return serve_catalog(app, peer, headers, uri, style, raw, shape, Some(plate)).await;
    }
    let Some(plate) = Plate::parse(&shape, &theme) else {
        return plain(
            StatusCode::NOT_FOUND,
            "unknown plate; use rounded|superellipse and light|dark",
        );
    };
    serve_icon(app, peer, headers, uri, style, raw, false, Some(plate)).await
}

async fn plated_catalog_icon(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
    Path((style, id, slug, shape, theme)): Path<(String, String, String, String, String)>,
) -> Response {
    let Some(plate) = Plate::parse(&shape, &theme) else {
        return plain(
            StatusCode::NOT_FOUND,
            "unknown plate; use rounded|superellipse and light|dark",
        );
    };
    serve_catalog(app, peer, headers, uri, style, id, slug, Some(plate)).await
}

#[allow(clippy::too_many_arguments)]
async fn serve_icon(
    app: Arc<App>,
    peer: SocketAddr,
    headers: HeaderMap,
    uri: Uri,
    style: String,
    raw: String,
    trailing_slash: bool,
    plate: Option<Plate>,
) -> Response {
    inc(&app.stats.requests);
    let Some(client) = admit(&app, peer, &headers) else {
        return rate_limited();
    };

    let Some(style) = Style::from_path(&style) else {
        return plain(StatusCode::NOT_FOUND, "unknown style; use color or mono");
    };
    let requested = raw.as_str();
    let raw = [".svg", ".png", ".webp"]
        .iter()
        .find_map(|ext| requested.strip_suffix(ext))
        .unwrap_or(requested);
    let if_none_match = headers.get(IF_NONE_MATCH).cloned();

    let policy = &app.cfg.policy;
    let Some(n) = normalize(raw, policy.collapse_private_suffixes, &policy.deny_suffixes) else {
        let req = Req {
            name: raw
                .chars()
                .take(MAX_INPUT_LEN)
                .collect::<String>()
                .to_lowercase(),
            valid: false,
            style,
            plate,
            if_none_match,
            nested: headers.contains_key(H_FETCH),
            gzip: accepts_gzip(&headers),
        };
        return fallback(&app, &req, FallbackReason::InvalidDomain);
    };
    if requested != n.registrable || uri.query().is_some() || trailing_slash {
        return redirect_to_canonical(&app, style, &n.registrable, plate).await;
    }
    let req = Req {
        name: n.registrable,
        valid: true,
        style,
        plate,
        if_none_match,
        nested: headers.contains_key(H_FETCH),
        gzip: accepts_gzip(&headers),
    };
    let domain = &req.name;
    if app.policy.is_blocked(&n.host, domain) {
        return fallback(&app, &req, FallbackReason::Blocklisted);
    }

    serve_key(&app, &req, client).await
}

async fn serve_key(app: &Arc<App>, req: &Req, client: ClientKey) -> Response {
    let domain = &req.name;
    if let Some(meta) = app.store.meta(domain).await {
        let stale = app.is_stale(&meta) || App::needs_refresh(&meta);
        if stale && !req.nested && app.limiter.escalate(client) {
            let _ = app.queue.push(domain, client);
        }
        return serve_meta(app, req, &meta, Source::Cache, stale).await;
    }

    if req.nested {
        return plain(StatusCode::NOT_FOUND, "not cached");
    }
    if !app.limiter.escalate(client) {
        inc(&app.stats.rate_limited);
        return rate_limited();
    }
    match app.queue.push(domain, client) {
        Enqueue::Rejected => fallback(app, req, FallbackReason::Overloaded),
        Enqueue::Pending(mut rx) => {
            let wait = Duration::from_millis(app.cfg.fetch.wait_ms);
            if !wait.is_zero()
                && tokio::time::timeout(wait, rx.wait_for(|done| *done))
                    .await
                    .is_ok()
                && let Some(meta) = app.store.meta(domain).await
            {
                return serve_meta(app, req, &meta, Source::Origin, false).await;
            }
            fallback(app, req, FallbackReason::Queued)
        }
    }
}

async fn catalog_icon(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
    Path((style, id, slug)): Path<(String, String, String)>,
) -> Response {
    if app.slugs.get(&id).is_none()
        && let Some(plate) = Plate::auto(&slug)
    {
        return serve_icon(app, peer, headers, uri, style, id, false, Some(plate)).await;
    }
    serve_catalog(app, peer, headers, uri, style, id, slug, None).await
}

#[allow(clippy::too_many_arguments)]
async fn serve_catalog(
    app: Arc<App>,
    peer: SocketAddr,
    headers: HeaderMap,
    uri: Uri,
    style: String,
    id: String,
    slug: String,
    plate: Option<Plate>,
) -> Response {
    inc(&app.stats.requests);
    let Some(client) = admit(&app, peer, &headers) else {
        return rate_limited();
    };
    let Some(style) = Style::from_path(&style) else {
        return plain(StatusCode::NOT_FOUND, "unknown style; use color or mono");
    };
    let Some(catalog) = app.slugs.get(&id) else {
        return plain(StatusCode::NOT_FOUND, "unknown catalog");
    };
    let wanted = slug
        .strip_suffix(".svg")
        .unwrap_or(&slug)
        .to_ascii_lowercase();
    if !crate::slugs::is_slug(&wanted) {
        return plain(StatusCode::NOT_FOUND, "unknown icon");
    }
    if wanted != slug || uri.query().is_some() {
        let suffix = plate.map_or("", Plate::suffix);
        let location = format!("/{}/{id}/{wanted}{suffix}", style.as_str());
        let key = crate::slugs::key(&id, &wanted);
        return redirect(
            StatusCode::MOVED_PERMANENTLY,
            &location,
            &app.profiles.canonical,
            style,
            &key,
        );
    }
    if !catalog.is_loaded() {
        let mut r = plain(StatusCode::SERVICE_UNAVAILABLE, "catalog is loading");
        r.headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from_static("5"));
        return r;
    }
    if !catalog.contains(&wanted) {
        return plain(StatusCode::NOT_FOUND, "unknown icon");
    }
    let req = Req {
        name: crate::slugs::key(&id, &wanted),
        valid: true,
        style,
        plate,
        if_none_match: headers.get(IF_NONE_MATCH).cloned(),
        nested: headers.contains_key(H_FETCH),
        gzip: accepts_gzip(&headers),
    };
    serve_key(&app, &req, client).await
}

fn json(body: bytes::Bytes, cache: &'static str) -> Response {
    let mut r = Response::new(Body::from(body));
    r.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    r.headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(cache));
    r
}

async fn stats(State(app): State<Arc<App>>) -> Response {
    let body = serde_json::json!({
        "instance": app.instance,
        "started_at": app.started_at,
        "icons": app.stats.requests.load(std::sync::atomic::Ordering::Relaxed),
    });
    let mut r = json(bytes::Bytes::from(body.to_string()), "no-store");
    r.headers_mut().insert(
        HeaderName::from_static(H_CDN_CACHE_CONTROL),
        HeaderValue::from_static("no-store"),
    );
    r
}

async fn catalogs_index(State(app): State<Arc<App>>) -> Response {
    json(app.slugs.summaries(), "public, max-age=300")
}

async fn catalog_listing(State(app): State<Arc<App>>, Path(file): Path<String>) -> Response {
    let Some(catalog) = file.strip_suffix(".json").and_then(|id| app.slugs.get(id)) else {
        return plain(StatusCode::NOT_FOUND, "unknown catalog");
    };
    if !catalog.is_loaded() {
        let mut r = plain(StatusCode::SERVICE_UNAVAILABLE, "catalog is loading");
        r.headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from_static("5"));
        return r;
    }
    json(
        catalog.listing(),
        "public, max-age=3600, stale-while-revalidate=86400",
    )
}

#[derive(Clone, Copy)]
struct Shown {
    slot: Slot,
    vector: &'static str,
    mono_source: Option<&'static str>,
}

async fn serve_meta(app: &App, req: &Req, meta: &Meta, source: Source, stale: bool) -> Response {
    if let Some(r) = status_reason(meta.status) {
        return fallback(app, req, r);
    }
    let shown = match req.style {
        Style::Mono => meta.mono.map(|origin| Shown {
            slot: Slot::Mono,
            vector: origin.as_str(),
            mono_source: meta.mono_source.map(MonoSource::as_str),
        }),
        Style::Color => meta.color.map(|origin| Shown {
            slot: Slot::Color,
            vector: origin.as_str(),
            mono_source: None,
        }),
    };
    let Some(shown) = shown else {
        let reason = if req.style == Style::Mono {
            FallbackReason::NoMonochrome
        } else {
            FallbackReason::NotFound
        };
        return fallback(app, req, reason);
    };
    let out = Out {
        source,
        domain: None,
        palette: &[],
        upstream: meta.upstream.as_deref(),
        provenance: meta.provenance.map(Provenance::as_str),
        license: meta.license.as_deref(),
        style: req.style,
        shown: Some(shown),
        reason: None,
        stale,
    };

    match app.store.image(meta, shown.slot).await {
        Some(icon) => {
            inc(&app.stats.served_cache);
            let out = Out {
                domain: icon.domain.clone(),
                palette: &icon.palette,
                ..out
            };
            image(
                &icon,
                req,
                &app.profiles.icon,
                &out,
                Some(&app.store.plated),
            )
        }
        None => {
            tracing::warn!(
                domain = meta.domain,
                "meta present but image missing; refetching"
            );
            fallback(app, req, FallbackReason::Queued)
        }
    }
}

fn status_reason(status: Status) -> Option<FallbackReason> {
    match status {
        Status::Ok => None,
        Status::NotFound => Some(FallbackReason::NotFound),
        Status::FetchFailed => Some(FallbackReason::FetchFailed),
        Status::TooNew => Some(FallbackReason::DomainTooNew),
        Status::Blocked => Some(FallbackReason::DnsBlocked),
    }
}

fn admit(app: &App, peer: SocketAddr, headers: &HeaderMap) -> Option<ClientKey> {
    let client = app.keyer.key(app.keyer.client_ip(peer.ip(), headers));
    if app.limiter.admit(client) {
        return Some(client);
    }
    inc(&app.stats.rate_limited);
    None
}

fn rate_limited() -> Response {
    let mut r = plain(StatusCode::TOO_MANY_REQUESTS, "rate limited");
    r.headers_mut()
        .insert(RETRY_AFTER, HeaderValue::from_static("1"));
    r
}

async fn redirect_to_canonical(
    app: &App,
    style: Style,
    domain: &str,
    plate: Option<Plate>,
) -> Response {
    inc(&app.stats.canonical_redirects);
    let suffix = plate.map_or("", Plate::suffix);
    let location = format!("/{}/{domain}{suffix}", style.as_str());
    redirect(
        StatusCode::MOVED_PERMANENTLY,
        &location,
        &app.profiles.canonical,
        style,
        domain,
    )
}

fn redirect(
    status: StatusCode,
    location: &str,
    profile: &CacheHeaders,
    style: Style,
    domain: &str,
) -> Response {
    let Ok(loc) = HeaderValue::from_str(location) else {
        return plain(StatusCode::BAD_REQUEST, "bad domain");
    };
    let mut r = Response::new(Body::empty());
    *r.status_mut() = status;
    r.headers_mut().insert(LOCATION, loc);
    let out = Out {
        source: Source::Cache,
        domain: HeaderValue::from_str(domain).ok(),
        palette: &[],
        upstream: None,
        provenance: None,
        license: None,
        style,
        shown: None,
        reason: None,
        stale: false,
    };
    decorate(&mut r, profile, &out);
    r
}

fn fallback(app: &App, req: &Req, reason: FallbackReason) -> Response {
    if req.nested {
        return plain(StatusCode::NOT_FOUND, "no icon");
    }
    app.stats.fallback(reason);
    let profile = if reason.is_transient() {
        &app.profiles.fallback_transient
    } else {
        &app.profiles.fallback_stable
    };
    let mono = req.style == Style::Mono;
    let shown = Shown {
        slot: if mono { Slot::Mono } else { Slot::Color },
        vector: "generated",
        mono_source: mono.then_some("generated"),
    };
    let icon = if reason.blocked_by().is_some() {
        app.warning_icon(req.style)
    } else {
        app.identicon(&req.name, req.style)
    };
    let out = Out {
        source: Source::Generated,
        domain: if req.valid {
            HeaderValue::from_str(&req.name).ok()
        } else {
            None
        },
        palette: &icon.palette,
        upstream: None,
        provenance: None,
        license: None,
        style: req.style,
        shown: Some(shown),
        reason: Some(reason),
        stale: false,
    };
    let etag = icon
        .etag
        .as_ref()
        .and_then(|e| e.to_str().ok())
        .and_then(|e| {
            HeaderValue::from_str(&format!("\"{}-{}\"", e.trim_matches('"'), reason.as_str())).ok()
        });
    let icon = icon.derived(icon.raw(), etag);
    image(&icon, req, profile, &out, None)
}

struct Out<'a> {
    source: Source,
    domain: Option<HeaderValue>,
    palette: &'a [(HeaderName, HeaderValue)],
    upstream: Option<&'a str>,
    provenance: Option<&'static str>,
    license: Option<&'a str>,
    style: Style,
    shown: Option<Shown>,
    reason: Option<FallbackReason>,
    stale: bool,
}

fn accepts_gzip(headers: &HeaderMap) -> bool {
    headers
        .get_all(ACCEPT_ENCODING)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .any(|token| {
            let mut parts = token.split(';');
            let name = parts.next().unwrap_or_default().trim();
            let refused = parts.any(|p| {
                p.trim()
                    .strip_prefix("q=")
                    .and_then(|q| q.trim().parse::<f32>().ok())
                    .is_some_and(|q| q == 0.0)
            });
            (name.eq_ignore_ascii_case("gzip") || name == "*") && !refused
        })
}

fn image(
    icon: &Icon,
    req: &Req,
    profile: &CacheHeaders,
    out: &Out<'_>,
    plates: Option<&crate::store::IconCache>,
) -> Response {
    if let Some(plate) = req.plate {
        let etag = icon
            .etag
            .as_ref()
            .and_then(|e| e.to_str().ok())
            .and_then(|e| {
                let base = e.trim_matches('"');
                HeaderValue::from_str(&format!("\"{base}{}\"", plate.suffix().replace('/', "-")))
                    .ok()
            });
        let compose = || {
            crate::plate::compose(&icon.raw(), plate, req.style, &icon.palette)
                .map(|data| Arc::new(icon.derived(data.into(), etag.clone())))
        };
        let plated = match (plates, &etag) {
            (Some(cache), Some(e)) => {
                let key = format!("{}{}", req.style.as_str(), e.to_str().unwrap_or_default());
                match cache.get(&key) {
                    Some(hit) => Some(hit),
                    None => compose().inspect(|p| cache.insert(key, p.clone())),
                }
            }
            _ => compose(),
        };
        if let Some(plated) = plated {
            return image(
                &plated,
                &Req {
                    plate: None,
                    ..req.clone()
                },
                profile,
                out,
                None,
            );
        }
    }
    let (body, etag, encoded) = match (icon.gzip(), req.gzip) {
        (Some(gz), true) => (gz.clone(), icon.gzip_etag(), true),
        _ => (icon.raw(), icon.etag.clone(), false),
    };
    let not_modified = match (&etag, &req.if_none_match) {
        (Some(e), Some(inm)) => match (inm.to_str(), e.to_str()) {
            (Ok(inm), Ok(e)) => etag_matches(inm, e),
            _ => false,
        },
        _ => false,
    };
    let mut r = if not_modified {
        let mut r = Response::new(Body::empty());
        *r.status_mut() = StatusCode::NOT_MODIFIED;
        r
    } else {
        let mut r = Response::new(Body::from(body));
        r.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static(SVG_MIME));
        if encoded {
            r.headers_mut()
                .insert(CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        }
        r
    };
    if icon.gzipped {
        r.headers_mut()
            .insert(VARY, HeaderValue::from_static("accept-encoding"));
    }
    if let Some(e) = etag {
        r.headers_mut().insert(ETAG, e);
    }
    decorate(&mut r, profile, out);
    r
}

fn decorate(r: &mut Response, profile: &CacheHeaders, out: &Out<'_>) {
    let h = r.headers_mut();
    let name = HeaderName::from_static;
    h.insert(CACHE_CONTROL, profile.browser.clone());
    h.insert(name(H_CDN_CACHE_CONTROL), profile.cdn.clone());
    h.insert(
        name(H_SOURCE),
        HeaderValue::from_static(out.source.as_str()),
    );
    h.insert(name(H_STYLE), HeaderValue::from_static(out.style.as_str()));
    if let Some(v) = &out.domain {
        h.insert(name(H_DOMAIN), v.clone());
    }
    for (k, v) in out.palette {
        h.insert(k.clone(), v.clone());
    }
    if let Some(shown) = out.shown {
        h.insert(name(H_VECTOR), HeaderValue::from_static(shown.vector));
        if let Some(v) = out.upstream.and_then(|u| HeaderValue::from_str(u).ok()) {
            h.insert(name(H_UPSTREAM), v);
        }
        if let Some(p) = out.provenance {
            h.insert(name(H_PROVENANCE), HeaderValue::from_static(p));
        }
        if let Some(v) = out.license.and_then(|l| HeaderValue::from_str(l).ok()) {
            h.insert(name(H_LICENSE), v);
        }
        if let Some(src) = shown.mono_source {
            h.insert(name(H_MONO_SOURCE), HeaderValue::from_static(src));
        }
    }
    if let Some(reason) = out.reason {
        h.insert(name(H_FALLBACK), HeaderValue::from_static("true"));
        h.insert(name(H_REASON), HeaderValue::from_static(reason.as_str()));
        if let Some(by) = reason.blocked_by() {
            h.insert(name(H_BLOCKED), HeaderValue::from_static(by));
        }
    }
    if out.stale {
        h.insert(name(H_STALE), HeaderValue::from_static("true"));
    }
}

pub(crate) fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    let bare = etag.trim_start_matches("W/");
    if_none_match
        .split(',')
        .map(str::trim)
        .any(|t| t == "*" || t.trim_start_matches("W/") == bare)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_etags() {
        assert!(etag_matches("\"abc\"", "\"abc\""));
        assert!(etag_matches("W/\"abc\", \"def\"", "\"abc\""));
        assert!(etag_matches("*", "\"abc\""));
        assert!(!etag_matches("\"abd\"", "\"abc\""));
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use crate::config::{Config, SandboxMode};
    use tower::ServiceExt;

    fn app() -> Arc<App> {
        let mut cfg = Config::default();
        cfg.fetch.wait_ms = 0;
        cfg.render.sandbox = SandboxMode::Off;
        cfg.rate_limit.requests_per_second = 0.0;
        App::new(cfg).unwrap()
    }

    async fn get(app: &Arc<App>, path: &str) -> Response {
        let mut req = http::Request::builder()
            .uri(path)
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([198, 51, 100, 7], 4000))));
        router(app.clone()).oneshot(req).await.unwrap()
    }

    fn location(r: &Response) -> &str {
        r.headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }

    #[cfg(feature = "site")]
    #[tokio::test]
    async fn serves_the_landing_page() {
        let app = app();
        let r = get(&app, "/").await;
        assert_eq!(r.status(), StatusCode::OK);
        let h = r.headers();
        assert_eq!(h.get(CONTENT_TYPE).unwrap(), "text/html; charset=utf-8");
        assert!(
            h.get(CONTENT_SECURITY_POLICY)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("'sha256-")
        );
        assert_eq!(h.get("x-robots-tag").unwrap(), "all");
        let etag = h.get(ETAG).unwrap().clone();

        let mut req = http::Request::builder()
            .uri("/")
            .header(IF_NONE_MATCH, etag)
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([198, 51, 100, 7], 4000))));
        let r = router(app.clone()).oneshot(req).await.unwrap();
        assert_eq!(r.status(), StatusCode::NOT_MODIFIED);

        let r = get(&app, "/favicon.svg").await;
        assert_eq!(r.headers().get(CONTENT_TYPE).unwrap(), SVG_MIME);

        let r = get(&app, "/color/example.com").await;
        let csp = r.headers().get(CONTENT_SECURITY_POLICY).unwrap();
        assert!(csp.to_str().unwrap().contains("sandbox"));
        assert_eq!(r.headers().get("x-robots-tag").unwrap(), "noindex");
    }

    #[tokio::test]
    async fn lets_other_origins_read_headers_and_timings() {
        let app = app();
        let r = get(&app, "/color/example.com").await;
        let h = r.headers();
        assert_eq!(h.get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "*");
        assert_eq!(h.get("timing-allow-origin").unwrap(), "*");
        assert!(
            h.get("server-timing")
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("app;dur=")
        );
        let exposed = h
            .get(ACCESS_CONTROL_EXPOSE_HEADERS)
            .unwrap()
            .to_str()
            .unwrap();
        for name in [
            "server-timing",
            "etag",
            "x-icon-light-plate",
            "cdn-cache-control",
        ] {
            assert!(exposed.contains(name), "{name}");
        }

        let mut req = http::Request::builder()
            .method(http::Method::OPTIONS)
            .uri("/color/example.com")
            .header("origin", "https://mail.example.org")
            .header("access-control-request-method", "GET")
            .header("access-control-request-headers", "if-none-match")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([198, 51, 100, 7], 4000))));
        let r = router(app.clone()).oneshot(req).await.unwrap();
        assert_eq!(r.status(), StatusCode::NO_CONTENT);
        let h = r.headers();
        assert_eq!(h.get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "*");
        assert!(
            h.get(ACCESS_CONTROL_ALLOW_METHODS)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("GET")
        );
        assert_eq!(h.get(ACCESS_CONTROL_ALLOW_HEADERS).unwrap(), "*");
    }

    #[tokio::test]
    async fn nested_fetches_never_queue_or_get_fallbacks() {
        let app = app();
        let mut req = http::Request::builder()
            .uri("/color/example.com")
            .header(H_FETCH, "1")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([198, 51, 100, 7], 4000))));
        let r = router(app.clone()).oneshot(req).await.unwrap();
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        assert_eq!(app.queue.len(), 0);
        assert_eq!(
            get(&app, "/color/example.com").await.status(),
            StatusCode::OK
        );
        assert_eq!(app.queue.len(), 1);
    }

    async fn json_of(app: &Arc<App>, path: &str) -> serde_json::Value {
        let r = get(app, path).await;
        let body = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn openapi_matches_the_router() {
        let mut cfg = Config::default();
        cfg.fetch.wait_ms = 0;
        cfg.render.sandbox = SandboxMode::Off;
        cfg.rate_limit.requests_per_second = 0.0;
        cfg.catalogs = vec![crate::slugs::tests::cfg()];
        let app = App::new(cfg).unwrap();
        app.slugs
            .get("selfhst")
            .unwrap()
            .load(crate::slugs::tests::INDEX.as_bytes());

        let spec = json_of(&app, "/openapi.json").await;
        assert_eq!(spec["openapi"], "3.1.0");
        assert_eq!(spec["info"]["version"], env!("CARGO_PKG_VERSION"));
        let mut paths: Vec<String> = spec["paths"].as_object().unwrap().keys().cloned().collect();
        paths.sort();
        let mut expected = vec![
            "/",
            "/catalogs.json",
            "/catalogs/{file}",
            "/healthz",
            "/llms.txt",
            "/openapi.json",
            "/robots.txt",
            "/{style}/{catalog}/{slug}",
            "/{style}/{catalog}/{slug}/{shape}",
            "/{style}/{catalog}/{slug}/{shape}/{theme}",
            "/{style}/{domain}",
            "/{style}/{domain}/{shape}",
            "/{style}/{domain}/{shape}/{theme}",
        ];
        if crate::site::ENABLED {
            expected.push("/sitemap.xml");
            expected.push("/config.example.toml");
            expected.push("/docker-compose.yml");
        }
        expected.sort();
        assert_eq!(paths, expected);

        for path in &paths {
            let concrete = path
                .replace("{style}", "color")
                .replace("{domain}", "example.com")
                .replace("{catalog}", "selfhst")
                .replace("{slug}", "jellyfin")
                .replace("{shape}", "superellipse")
                .replace("{theme}", "dark")
                .replace("{file}", "selfhst.json");
            let status = get(&app, &concrete).await.status();
            assert!(status.is_success(), "{concrete}: {status}");
        }

        let catalog = &spec["paths"]["/{style}/{catalog}/{slug}"]["get"]["parameters"][1];
        assert_eq!(catalog["schema"]["enum"][0], "selfhst");
        let reasons = &spec["components"]["responses"]["Icon"]["headers"]["X-Icon-Fallback-Reason"]
            ["schema"]["enum"];
        assert_eq!(reasons.as_array().unwrap().len(), FallbackReason::ALL.len());

        if !crate::site::ENABLED {
            let r = get(&app, "/").await;
            assert_eq!(r.headers().get(CONTENT_TYPE).unwrap(), "application/json");
            assert_eq!(json_of(&app, "/").await, spec);
        }

        let plain = App::new(Config::default()).unwrap();
        let spec = json_of(&plain, "/openapi.json").await;
        assert!(spec["paths"].get("/{style}/{catalog}/{slug}").is_none());
    }

    #[tokio::test]
    async fn serves_catalog_icons_by_slug() {
        let mut cfg = Config::default();
        cfg.fetch.wait_ms = 0;
        cfg.render.sandbox = SandboxMode::Off;
        cfg.rate_limit.requests_per_second = 0.0;
        cfg.catalogs = vec![crate::slugs::tests::cfg()];
        let app = App::new(cfg).unwrap();

        let r = get(&app, "/color/selfhst/jellyfin").await;
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
        app.slugs
            .get("selfhst")
            .unwrap()
            .load(crate::slugs::tests::INDEX.as_bytes());

        let r = get(&app, "/color/selfhst/jellyfin").await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers().get(CONTENT_TYPE).unwrap(), SVG_MIME);
        assert_eq!(r.headers().get(H_REASON).unwrap(), "queued");
        assert!(r.headers().get("x-icon-light-plate").is_some());

        let r = get(&app, "/mono/selfhst/Jellyfin.svg").await;
        assert_eq!(r.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(location(&r), "/mono/selfhst/jellyfin");

        let r = get(&app, "/color/selfhst/jellyfin/superellipse").await;
        assert_eq!(r.status(), StatusCode::OK);
        let body = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains("prefers-color-scheme:dark"));

        let r = get(&app, "/mono/selfhst/Jellyfin/rounded").await;
        assert_eq!(r.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(location(&r), "/mono/selfhst/jellyfin/rounded");

        for path in [
            "/color/selfhst/missing",
            "/color/nope/jellyfin",
            "/purple/selfhst/jellyfin",
            "/color/selfhst/a.b",
        ] {
            assert_eq!(
                get(&app, path).await.status(),
                StatusCode::NOT_FOUND,
                "{path}"
            );
        }

        let r = get(&app, "/catalogs.json").await;
        let body = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(list[0]["id"], "selfhst");
        assert_eq!(list[0]["count"], 3);
        assert_eq!(list[0]["license"], "CC-BY-4.0");

        let r = get(&app, "/catalogs/selfhst.json").await;
        assert_eq!(r.status(), StatusCode::OK);
        let body = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        let listing: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(listing["items"].as_array().unwrap().len(), 3);
        assert_eq!(
            get(&app, "/catalogs/nope.json").await.status(),
            StatusCode::NOT_FOUND
        );
    }

    async fn get_with(app: &Arc<App>, path: &str, headers: &[(&str, &str)]) -> Response {
        let mut b = http::Request::builder().uri(path);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        let mut req = b.body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([198, 51, 100, 7], 4000))));
        router(app.clone()).oneshot(req).await.unwrap()
    }

    #[test]
    fn missing_mono_from_an_older_build_is_refreshed_once() {
        let mut meta = crate::store::Meta::new("example.com", crate::store::Status::Ok);
        meta.color = Some(crate::vector::VectorOrigin::Original);
        meta.palette = Some(crate::palette::analyze(&image::RgbaImage::new(4, 4)));
        meta.palette_rev = crate::palette::REVISION;
        assert!(App::needs_refresh(&meta));
        meta.mono_rev = crate::render::MONO_REVISION;
        assert!(!App::needs_refresh(&meta));
        meta.mono_rev = 0;
        meta.mono = Some(crate::vector::VectorOrigin::Traced);
        assert!(!App::needs_refresh(&meta));
    }

    #[tokio::test]
    async fn stored_icons_go_out_gzipped_and_fallbacks_raw() {
        let app = app();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><path fill="#000000" d="M0 0h128v128H0z"/></svg>"##;
        let mut meta = crate::store::Meta::new("example.com", crate::store::Status::Ok);
        meta.color = Some(crate::vector::VectorOrigin::Original);
        meta.etags.insert("color.svg".into(), "abc".into());
        app.store
            .put(
                meta,
                vec![(crate::contract::Slot::Color, svg.as_bytes().to_vec())],
            )
            .await;

        let r = get_with(
            &app,
            "/color/example.com",
            &[("accept-encoding", "br, gzip")],
        )
        .await;
        assert_eq!(r.headers().get(CONTENT_ENCODING).unwrap(), "gzip");
        assert_eq!(r.headers().get(VARY).unwrap(), "accept-encoding");
        assert_eq!(r.headers().get(ETAG).unwrap(), "\"abc-gz\"");
        let body = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            crate::store::gunzip(&body).unwrap().as_ref(),
            svg.as_bytes()
        );

        let r = get_with(
            &app,
            "/color/example.com",
            &[("accept-encoding", "gzip;q=0")],
        )
        .await;
        assert!(r.headers().get(CONTENT_ENCODING).is_none());
        assert_eq!(r.headers().get(ETAG).unwrap(), "\"abc\"");
        let body = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), svg.as_bytes());

        let r = get_with(
            &app,
            "/color/example.com",
            &[("accept-encoding", "gzip"), ("if-none-match", "\"abc-gz\"")],
        )
        .await;
        assert_eq!(r.status(), StatusCode::NOT_MODIFIED);

        let r = get_with(
            &app,
            "/color/example.com/rounded/dark",
            &[("accept-encoding", "gzip")],
        )
        .await;
        assert_eq!(r.headers().get(CONTENT_ENCODING).unwrap(), "gzip");
        assert_eq!(r.headers().get(ETAG).unwrap(), "\"abc-rounded-dark-gz\"");
        let body = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        let plated = String::from_utf8(crate::store::gunzip(&body).unwrap().to_vec()).unwrap();
        assert!(plated.contains("M28.8 0h70.4"), "{plated}");
        assert!(app.store.plated.contains_key("color\"abc-rounded-dark\""));

        let r = get_with(
            &app,
            "/color/not-cached.com",
            &[("accept-encoding", "gzip")],
        )
        .await;
        assert!(r.headers().get(CONTENT_ENCODING).is_none());
        assert!(r.headers().get(VARY).is_none());
    }

    #[tokio::test]
    async fn prerenders_inline_catalogs_to_disk_and_frees_the_sources() {
        let dir = std::env::temp_dir().join(format!("favicons-pre-{}", rand::random::<u64>()));
        let mut cfg = Config::default();
        cfg.fetch.wait_ms = 0;
        cfg.render.sandbox = SandboxMode::Off;
        cfg.rate_limit.requests_per_second = 0.0;
        cfg.cache.storage = format!("file://{}", dir.display());
        cfg.catalogs = vec![crate::config::SlugCatalog {
            id: "emoji".into(),
            format: crate::config::IndexFormat::Iconify,
            ..crate::slugs::tests::cfg()
        }];
        let app = App::new(cfg).unwrap();
        let catalog = app.slugs.get("emoji").unwrap().clone();
        catalog.load(
            br##"{"width":32,"height":32,"icons":{"red-square":{"body":"<path fill=\"#e5484d\" d=\"M4 4h24v24H4z\"/>"}}}"##,
        );
        assert_eq!(catalog.sources(false).len(), 1);
        app.prerender(&catalog, false, 4).await;
        assert!(catalog.sources(false).is_empty());
        let meta = app.store.meta("@emoji.red-square").await.unwrap();
        assert!(meta.source.is_some() && meta.palette.is_some() && !app.is_stale(&meta));
        assert!(dir.join("v4/@emoji.red-square/color.svg.gz").exists());

        let r = get_with(
            &app,
            "/color/emoji/red-square",
            &[("accept-encoding", "gzip")],
        )
        .await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers().get(CONTENT_ENCODING).unwrap(), "gzip");
        assert_eq!(r.headers().get("x-icon-provenance").unwrap(), "catalog");

        app.prerender(&catalog, false, 4).await;
        catalog.load(
            br##"{"width":32,"height":32,"icons":{"red-square":{"body":"<path fill=\"#e5484d\" d=\"M4 4h24v24H4z\"/>"}}}"##,
        );
        app.prerender(&catalog, false, 4).await;
        assert!(catalog.sources(false).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn draws_icons_on_themed_plates() {
        let app = app();
        let r = get(&app, "/color/example.com/rounded/dark").await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers().get(CONTENT_TYPE).unwrap(), SVG_MIME);
        let etag = r.headers().get(ETAG).unwrap().to_str().unwrap().to_string();
        assert!(etag.ends_with("-rounded-dark\""), "{etag}");
        let body = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        let svg = String::from_utf8(body.to_vec()).unwrap();
        assert!(svg.contains("<clipPath id=\"c-"), "{svg}");
        assert!(svg.contains("id=\"g0-"), "{svg}");

        let r = get(&app, "/mono/example.com/superellipse/light").await;
        assert_eq!(r.status(), StatusCode::OK);

        let r = get(&app, "/color/WWW.Example.com/superellipse/dark").await;
        assert_eq!(r.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(location(&r), "/color/example.com/superellipse/dark");

        let r = get(&app, "/color/example.com/rounded").await;
        assert_eq!(r.status(), StatusCode::OK);
        let etag = r.headers().get(ETAG).unwrap().to_str().unwrap().to_string();
        assert!(etag.ends_with("-rounded\""), "{etag}");
        let csp = r
            .headers()
            .get(CONTENT_SECURITY_POLICY)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let body = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        let svg = String::from_utf8(body.to_vec()).unwrap();
        assert!(svg.contains("@media (prefers-color-scheme:dark)"), "{svg}");
        assert_eq!(
            csp,
            "default-src 'none'; style-src 'unsafe-inline'; script-src 'none'; sandbox"
        );

        let r = get(&app, "/mono/WWW.Example.com/superellipse").await;
        assert_eq!(r.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(location(&r), "/mono/example.com/superellipse");

        for path in [
            "/color/example.com/circle/dark",
            "/color/example.com/rounded/blue",
            "/color/example.com/circle",
        ] {
            assert_eq!(
                get(&app, path).await.status(),
                StatusCode::NOT_FOUND,
                "{path}"
            );
        }
    }

    #[cfg(feature = "site")]
    #[tokio::test]
    async fn landing_can_be_turned_off() {
        let mut cfg = Config::default();
        cfg.render.sandbox = SandboxMode::Off;
        cfg.server.landing = false;
        let app = App::new(cfg).unwrap();
        let r = get(&app, "/").await;
        assert_eq!(r.headers().get(CONTENT_TYPE).unwrap(), "application/json");
        assert_eq!(
            get(&app, "/favicon.svg").await.status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn redirects_every_non_canonical_form() {
        let app = app();
        for (path, target) in [
            (
                "/color/super-domain.evil-example.com",
                "/color/evil-example.com",
            ),
            ("/color/EXAMPLE.com", "/color/example.com"),
            ("/color/example.com.", "/color/example.com"),
            ("/mono/example.com.svg", "/mono/example.com"),
            ("/color/example.com?size=64", "/color/example.com"),
            ("/color/example.com?", "/color/example.com"),
            ("/mono/example.com/", "/mono/example.com"),
            (
                "/color/https%3A%2F%2Fwww.example.co.uk%2Fpath",
                "/color/example.co.uk",
            ),
            ("/color/u-123.github.io", "/color/github.io"),
        ] {
            let r = get(&app, path).await;
            assert_eq!(r.status(), StatusCode::MOVED_PERMANENTLY, "{path}");
            assert_eq!(location(&r), target, "{path}");
            let cc = r.headers().get(CACHE_CONTROL).unwrap().to_str().unwrap();
            assert!(cc.contains("immutable"), "{path}: {cc}");
        }
    }

    #[tokio::test]
    async fn serves_canonical_paths_without_redirect() {
        let app = app();
        let r = get(&app, "/color/example.com").await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers().get(CONTENT_TYPE).unwrap(), SVG_MIME);
        assert_eq!(r.headers().get(H_REASON).unwrap(), "queued");
        let etag = r.headers().get(ETAG).unwrap().to_str().unwrap();
        assert!(etag.ends_with("-queued\""), "{etag}");
        let r = get(&app, "/mono/127.0.0.1").await;
        let etag = r.headers().get(ETAG).unwrap().to_str().unwrap();
        assert!(etag.ends_with("-invalid-domain\""), "{etag}");
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers().get(H_REASON).unwrap(), "invalid-domain");
        assert_eq!(r.headers().get(H_STYLE).unwrap(), "mono");
    }

    #[tokio::test]
    async fn sends_palette_headers_and_exposes_them() {
        let app = app();
        for path in ["/color/example.com", "/mono/example.com"] {
            let r = get(&app, path).await;
            assert_eq!(r.status(), StatusCode::OK);
            let h = r.headers();
            assert_eq!(h.get("x-icon-shape").unwrap(), "square");
            assert_eq!(h.get("x-icon-fill").unwrap(), "bleed");
            assert!(EXPOSED.contains("x-icon-fill"));
            for name in [
                "x-icon-accent",
                "x-icon-light-plate",
                "x-icon-light-text",
                "x-icon-light-mono",
                "x-icon-dark-plate",
                "x-icon-dark-text",
                "x-icon-dark-mono",
                "x-icon-light-mono-plate",
                "x-icon-dark-mono-plate",
            ] {
                let v = h.get(name).unwrap().to_str().unwrap();
                assert!(
                    crate::palette::Rgb::parse(v).is_some(),
                    "{path} {name} = {v}"
                );
                assert!(EXPOSED.contains(name), "{name} not exposed");
            }
        }
        let r = get(&app, "/color/example.com").await;
        let exposed = r.headers().get(ACCESS_CONTROL_EXPOSE_HEADERS).unwrap();
        assert!(exposed.to_str().unwrap().contains("x-icon-dark-plate"));
        assert_eq!(
            get(&app, "/v1/palette/example.com").await.status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn rejects_unknown_styles_and_old_paths() {
        let app = app();
        assert_eq!(
            get(&app, "/purple/example.com").await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(&app, "/example.com").await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(&app, "/v1/icon/color/example.com").await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(&app, "/color/a/b").await.status(),
            StatusCode::NOT_FOUND
        );
    }
}
