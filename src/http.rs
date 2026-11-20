use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use bytes::Bytes;
use http::header::{
    ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG,
    IF_NONE_MATCH, LOCATION, REFERRER_POLICY, RETRY_AFTER, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};

use crate::app::App;
use crate::config::CacheProfile;
use crate::contract::{
    FallbackReason, H_BLOCKED, H_CDN_CACHE_CONTROL, H_DOMAIN, H_FALLBACK, H_LICENSE, H_MONO_SOURCE,
    H_PROVENANCE, H_REASON, H_SOURCE, H_STALE, H_STYLE, H_UPSTREAM, H_VECTOR, MonoSource,
    Provenance, SVG_MIME, Slot, Source, Style,
};
use crate::domain::{MAX_INPUT_LEN, normalize};
use crate::metrics::inc;
use crate::queue::Enqueue;
use crate::store::{Meta, Status, object_name};

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/v1/icon/{style}/{domain}", get(icon))
        .route("/v1/icon/{style}/{domain}/", get(icon_slash))
        .route("/healthz", get(|| async { "ok" }))
        .fallback(|| async { plain(StatusCode::NOT_FOUND, "not found") })
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .layer(middleware::map_response(security_headers))
        .with_state(app)
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

struct Req {
    domain: Option<String>,
    seed: String,
    style: Style,
    if_none_match: Option<String>,
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
    let client = app.keyer.key(app.keyer.client_ip(peer.ip(), &headers));
    if !app.limiter.check(client) {
        inc(&app.stats.rate_limited);
        let mut r = plain(StatusCode::TOO_MANY_REQUESTS, "rate limited");
        r.headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from_static("1"));
        return r;
    }

    let Some(style) = Style::from_path(&style) else {
        return plain(StatusCode::NOT_FOUND, "unknown style; use color or mono");
    };
    let requested = raw.as_str();
    let raw = [".svg", ".png", ".webp"]
        .iter()
        .find_map(|ext| requested.strip_suffix(ext))
        .unwrap_or(requested);
    let mut req = Req {
        domain: None,
        seed: raw
            .chars()
            .take(MAX_INPUT_LEN)
            .collect::<String>()
            .to_lowercase(),
        style,
        if_none_match: headers
            .get(IF_NONE_MATCH)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
    };

    let policy = &app.cfg.policy;
    let Some(n) = normalize(raw, policy.collapse_private_suffixes, &policy.deny_suffixes) else {
        return fallback(&app, &req, FallbackReason::InvalidDomain);
    };
    let domain = n.registrable;
    req.seed.clone_from(&domain);
    req.domain = Some(domain.clone());
    if requested != domain || uri.query().is_some() || trailing_slash {
        return redirect_to_canonical(&app, style, &domain).await;
    }
    if app.policy.is_blocked(&n.host, &domain) {
        return fallback(&app, &req, FallbackReason::Blocklisted);
    }

    if let Some(meta) = app.store.meta(&domain).await {
        let stale = app.is_stale(&meta);
        if stale {
            let _ = app.queue.push(&domain, client);
        }
        return serve_meta(&app, &req, &meta, Source::Cache, stale).await;
    }

    match app.queue.push(&domain, client) {
        Enqueue::Rejected => fallback(&app, &req, FallbackReason::Overloaded),
        Enqueue::Pending(mut rx) => {
            let wait = Duration::from_millis(app.cfg.fetch.wait_ms);
            if !wait.is_zero()
                && tokio::time::timeout(wait, rx.wait_for(|done| *done))
                    .await
                    .is_ok()
                && let Some(meta) = app.store.meta(&domain).await
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
    let reason = match meta.status {
        Status::Ok => None,
        Status::NotFound => Some(FallbackReason::NotFound),
        Status::FetchFailed => Some(FallbackReason::FetchFailed),
        Status::TooNew => Some(FallbackReason::DomainTooNew),
        Status::Blocked => Some(FallbackReason::DnsBlocked),
    };
    if let Some(r) = reason {
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
    let etag = meta
        .etags
        .get(&object_name(shown.slot))
        .map(|e| format!("\"{e}\""));
    let out = Out {
        source,
        domain: Some(&meta.domain),
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
        decorate(&mut r, &app.cfg.http_cache.redirect, &out);
        return r;
    }

    match app.store.image(&meta.domain, shown.slot).await {
        Some(data) => {
            inc(&app.stats.served_cache);
            image(data, etag, req, &app.cfg.http_cache.icon, &out)
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
            &app.cfg.http_cache.redirect,
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
        &app.cfg.http_cache.canonical,
        style,
        domain,
    )
}

fn redirect(
    app: &App,
    status: StatusCode,
    location: &str,
    profile: &CacheProfile,
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
        domain: Some(domain),
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
    let hc = &app.cfg.http_cache;
    let profile = if reason.is_transient() {
        &hc.fallback_transient
    } else {
        &hc.fallback_stable
    };
    let mono = req.style == Style::Mono;
    let shown = Shown {
        slot: if mono { Slot::Mono } else { Slot::Color },
        vector: "generated",
        mono_source: mono.then_some("generated"),
    };
    let out = Out {
        source: Source::Generated,
        domain: req.domain.as_deref(),
        upstream: None,
        provenance: None,
        license: None,
        style: req.style,
        shown: Some(shown),
        reason: Some(reason),
        stale: false,
    };
    let data = if reason.blocked_by().is_some() {
        app.warning_icon(req.style)
    } else {
        app.identicon(&req.seed, req.style)
    };
    let etag = format!("\"g-{}\"", &blake3::hash(&data).to_hex()[..20]);
    image(data, Some(etag), req, profile, &out)
}

struct Out<'a> {
    source: Source,
    domain: Option<&'a str>,
    upstream: Option<&'a str>,
    provenance: Option<&'static str>,
    license: Option<&'a str>,
    style: Style,
    shown: Option<Shown>,
    reason: Option<FallbackReason>,
    stale: bool,
}

fn image(
    data: Bytes,
    etag: Option<String>,
    req: &Req,
    profile: &CacheProfile,
    out: &Out<'_>,
) -> Response {
    let not_modified =
        matches!((&etag, &req.if_none_match), (Some(e), Some(inm)) if etag_matches(inm, e));
    let mut r = if not_modified {
        let mut r = Response::new(Body::empty());
        *r.status_mut() = StatusCode::NOT_MODIFIED;
        r
    } else {
        let mut r = Response::new(Body::from(data));
        r.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static(SVG_MIME));
        r
    };
    if let Some(e) = etag.and_then(|e| HeaderValue::from_str(&e).ok()) {
        r.headers_mut().insert(ETAG, e);
    }
    decorate(&mut r, profile, out);
    r
}

fn decorate(r: &mut Response, profile: &CacheProfile, out: &Out<'_>) {
    let h = r.headers_mut();
    let name = HeaderName::from_static;
    if let Ok(v) = HeaderValue::from_str(&profile.browser) {
        h.insert(CACHE_CONTROL, v);
    }
    if let Ok(v) = HeaderValue::from_str(&profile.cdn) {
        h.insert(name(H_CDN_CACHE_CONTROL), v);
    }
    h.insert(
        name(H_SOURCE),
        HeaderValue::from_static(out.source.as_str()),
    );
    h.insert(name(H_STYLE), HeaderValue::from_static(out.style.as_str()));
    if let Some(v) = out.domain.and_then(|d| HeaderValue::from_str(d).ok()) {
        h.insert(name(H_DOMAIN), v);
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
