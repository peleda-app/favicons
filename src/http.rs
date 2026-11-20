use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use http::header::{
    ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, CACHE_CONTROL,
    CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, IF_NONE_MATCH, LOCATION, REFERRER_POLICY,
    RETRY_AFTER, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};

use crate::app::App;
use crate::config::{CacheProfile, HttpCache};
use crate::contract::{
    FallbackReason, H_BLOCKED, H_CDN_CACHE_CONTROL, H_DOMAIN, H_FALLBACK, H_LICENSE, H_MONO_SOURCE,
    H_PROVENANCE, H_REASON, H_SOURCE, H_STALE, H_STYLE, H_UPSTREAM, H_VECTOR, MonoSource,
    Provenance, SVG_MIME, Slot, Source, Style,
};
use crate::domain::{MAX_INPUT_LEN, normalize};
use crate::metrics::inc;
use crate::queue::{ClientKey, Enqueue};
use crate::store::{Icon, Meta, Status};

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/v1/icon/{style}/{domain}", get(icon))
        .route("/v1/icon/{style}/{domain}/", get(icon_slash))
        .route("/.well-known/acme-challenge/{token}", get(acme_challenge))
        .route("/healthz", get(|| async { "ok" }))
        .fallback(|| async { plain(StatusCode::NOT_FOUND, "not found") })
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .layer(middleware::map_response(security_headers))
        .with_state(app)
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
    match tokio::time::timeout(timeout, next.run(req)).await {
        Ok(r) => r,
        Err(_) => plain(StatusCode::SERVICE_UNAVAILABLE, "timeout"),
    }
}

const EXPOSED: &str = "etag, x-icon-source, x-icon-domain, x-icon-style, x-icon-vector, \
     x-icon-provenance, x-icon-license, x-icon-upstream, x-icon-mono-source, x-icon-fallback, \
     x-icon-fallback-reason, x-icon-stale, x-icon-blocked, x-icon-shape, x-icon-fill, x-icon-accent, \
     x-icon-light-plate, x-icon-light-text, x-icon-light-mono, x-icon-light-mono-plate, \
     x-icon-dark-plate, x-icon-dark-text, x-icon-dark-mono, x-icon-dark-mono-plate";

async fn security_headers(mut res: Response) -> Response {
    let h = res.headers_mut();
    h.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; style-src 'none'; script-src 'none'; sandbox",
        ),
    );
    h.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("cross-origin"),
    );
    h.insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert(
        ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static(EXPOSED),
    );
    h.insert(
        HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex"),
    );
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
    redirect: CacheHeaders,
    canonical: CacheHeaders,
    fallback_transient: CacheHeaders,
    fallback_stable: CacheHeaders,
}

impl Profiles {
    pub fn new(c: &HttpCache) -> anyhow::Result<Self> {
        Ok(Self {
            icon: CacheHeaders::new("icon", &c.icon)?,
            redirect: CacheHeaders::new("redirect", &c.redirect)?,
            canonical: CacheHeaders::new("canonical", &c.canonical)?,
            fallback_transient: CacheHeaders::new("fallback_transient", &c.fallback_transient)?,
            fallback_stable: CacheHeaders::new("fallback_stable", &c.fallback_stable)?,
        })
    }
}

struct Req {
    name: String,
    valid: bool,
    style: Style,
    if_none_match: Option<HeaderValue>,
}

async fn icon(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
    Path((style, raw)): Path<(String, String)>,
) -> Response {
    serve_icon(app, peer, headers, uri, style, raw, false).await
}

async fn icon_slash(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
    Path((style, raw)): Path<(String, String)>,
) -> Response {
    serve_icon(app, peer, headers, uri, style, raw, true).await
}

async fn serve_icon(
    app: Arc<App>,
    peer: SocketAddr,
    headers: HeaderMap,
    uri: Uri,
    style: String,
    raw: String,
    trailing_slash: bool,
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
            if_none_match,
        };
        return fallback(&app, &req, FallbackReason::InvalidDomain);
    };
    if requested != n.registrable || uri.query().is_some() || trailing_slash {
        return redirect_to_canonical(&app, style, &n.registrable).await;
    }
    let req = Req {
        name: n.registrable,
        valid: true,
        style,
        if_none_match,
    };
    let domain = &req.name;
    if app.policy.is_blocked(&n.host, domain) {
        return fallback(&app, &req, FallbackReason::Blocklisted);
    }

    if let Some(meta) = app.store.meta(domain).await {
        let stale = app.is_stale(&meta)
            || (meta.status == Status::Ok && meta.color.is_some() && meta.palette.is_none());
        if stale {
            let _ = app.queue.push(domain, client);
        }
        return serve_meta(&app, &req, &meta, Source::Cache, stale).await;
    }

    match app.queue.push(domain, client) {
        Enqueue::Rejected => fallback(&app, &req, FallbackReason::Overloaded),
        Enqueue::Pending(mut rx) => {
            let wait = Duration::from_millis(app.cfg.fetch.wait_ms);
            if !wait.is_zero()
                && tokio::time::timeout(wait, rx.wait_for(|done| *done))
                    .await
                    .is_ok()
                && let Some(meta) = app.store.meta(domain).await
            {
                return serve_meta(&app, &req, &meta, Source::Origin, false).await;
            }
            fallback(&app, &req, FallbackReason::Queued)
        }
    }
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

    if app.cfg.cache.redirect_to_public_url
        && app.store.has_backend()
        && let Some(url) = app.store.public_url(&meta.domain, shown.slot)
        && let Ok(loc) = HeaderValue::from_str(&url)
    {
        inc(&app.stats.served_redirect);
        let mut r = Response::new(Body::empty());
        *r.status_mut() = StatusCode::FOUND;
        r.headers_mut().insert(LOCATION, loc);
        let palette = meta.palette.map(|p| p.headers()).unwrap_or_default();
        let out = Out {
            domain: HeaderValue::from_str(&meta.domain).ok(),
            palette: &palette,
            ..out
        };
        decorate(&mut r, &app.profiles.redirect, &out);
        return r;
    }

    match app.store.image(meta, shown.slot).await {
        Some(icon) => {
            inc(&app.stats.served_cache);
            let out = Out {
                domain: icon.domain.clone(),
                palette: &icon.palette,
                ..out
            };
            image(&icon, req, &app.profiles.icon, &out)
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
    if app.limiter.check(client) {
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

async fn redirect_to_canonical(app: &App, style: Style, domain: &str) -> Response {
    let slot = match style {
        Style::Color => Slot::Color,
        Style::Mono => Slot::Mono,
    };
    if app.cfg.cache.redirect_to_public_url
        && app.store.has_backend()
        && let Some(meta) = app.store.meta(domain).await
        && meta.status == Status::Ok
        && match slot {
            Slot::Color => meta.color.is_some(),
            Slot::Mono => meta.mono.is_some(),
        }
        && let Some(url) = app.store.public_url(domain, slot)
    {
        return redirect(
            app,
            StatusCode::FOUND,
            &url,
            &app.profiles.redirect,
            style,
            domain,
        );
    }
    inc(&app.stats.canonical_redirects);
    let location = format!("/v1/icon/{}/{domain}", style.as_str());
    redirect(
        app,
        StatusCode::MOVED_PERMANENTLY,
        &location,
        &app.profiles.canonical,
        style,
        domain,
    )
}

fn redirect(
    app: &App,
    status: StatusCode,
    location: &str,
    profile: &CacheHeaders,
    style: Style,
    domain: &str,
) -> Response {
    let Ok(loc) = HeaderValue::from_str(location) else {
        return plain(StatusCode::BAD_REQUEST, "bad domain");
    };
    if status == StatusCode::FOUND {
        inc(&app.stats.served_redirect);
    }
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
    image(&icon, req, profile, &out)
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

fn image(icon: &Icon, req: &Req, profile: &CacheHeaders, out: &Out<'_>) -> Response {
    let not_modified = match (&icon.etag, &req.if_none_match) {
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
        let mut r = Response::new(Body::from(icon.data.clone()));
        r.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static(SVG_MIME));
        r
    };
    if let Some(e) = &icon.etag {
        r.headers_mut().insert(ETAG, e.clone());
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

fn etag_matches(if_none_match: &str, etag: &str) -> bool {
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

    #[tokio::test]
    async fn redirects_every_non_canonical_form() {
        let app = app();
        for (path, target) in [
            (
                "/v1/icon/color/super-domain.evil-example.com",
                "/v1/icon/color/evil-example.com",
            ),
            ("/v1/icon/color/EXAMPLE.com", "/v1/icon/color/example.com"),
            ("/v1/icon/color/example.com.", "/v1/icon/color/example.com"),
            ("/v1/icon/mono/example.com.svg", "/v1/icon/mono/example.com"),
            (
                "/v1/icon/color/example.com?size=64",
                "/v1/icon/color/example.com",
            ),
            ("/v1/icon/color/example.com?", "/v1/icon/color/example.com"),
            ("/v1/icon/mono/example.com/", "/v1/icon/mono/example.com"),
            (
                "/v1/icon/color/https%3A%2F%2Fwww.example.co.uk%2Fpath",
                "/v1/icon/color/example.co.uk",
            ),
            ("/v1/icon/color/u-123.github.io", "/v1/icon/color/github.io"),
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
        let r = get(&app, "/v1/icon/color/example.com").await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers().get(CONTENT_TYPE).unwrap(), SVG_MIME);
        assert_eq!(r.headers().get(H_REASON).unwrap(), "queued");
        let r = get(&app, "/v1/icon/mono/127.0.0.1").await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers().get(H_REASON).unwrap(), "invalid-domain");
        assert_eq!(r.headers().get(H_STYLE).unwrap(), "mono");
    }

    #[tokio::test]
    async fn sends_palette_headers_and_exposes_them() {
        let app = app();
        for path in ["/v1/icon/color/example.com", "/v1/icon/mono/example.com"] {
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
        let r = get(&app, "/v1/icon/color/example.com").await;
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
            get(&app, "/v1/icon/purple/example.com").await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(&app, "/v1/icon/example.com").await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(&app, "/v1/icon/color/a/b").await.status(),
            StatusCode::NOT_FOUND
        );
    }
}
