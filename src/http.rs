use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use bytes::Bytes;
use http::header::{
    ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG,
    IF_NONE_MATCH, LOCATION, REFERRER_POLICY, RETRY_AFTER, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use serde::Deserialize;

use crate::app::App;
use crate::config::CacheProfile;
use crate::contract::{
    FallbackReason, H_BLOCKED, H_CDN_CACHE_CONTROL, H_DOMAIN, H_FALLBACK, H_MONO_SOURCE, H_REASON,
    H_SIZE, H_SOURCE, H_STALE, H_STYLE, H_THEME, MonoSource, OutputFormat, Slot, Source, Style,
    Theme, pick_available, snap_size,
};
use crate::domain::{MAX_INPUT_LEN, normalize};
use crate::metrics::inc;
use crate::queue::Enqueue;
use crate::store::{Meta, Status, object_name};

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/v1/icon/{domain}", get(icon))
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
                let (blocked, aliases) = app.policy.counts();
                app.stats
                    .render(app.queue.len(), app.queue.running(), blocked, aliases)
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
        HeaderValue::from_static("default-src 'none'; sandbox"),
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

#[derive(Deserialize)]
pub struct IconQuery {
    size: Option<String>,
    theme: Option<String>,
    style: Option<String>,
    fallback: Option<String>,
}

struct Req {
    domain: Option<String>,
    seed: String,
    size: u32,
    theme: Theme,
    style: Style,
    want_fallback: bool,
    if_none_match: Option<String>,
}

async fn icon(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(raw): Path<String>,
    Query(q): Query<IconQuery>,
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

    let raw = [".png", ".webp"]
        .iter()
        .find_map(|ext| raw.strip_suffix(ext))
        .unwrap_or(&raw);
    let mut req = Req {
        domain: None,
        seed: raw
            .chars()
            .take(MAX_INPUT_LEN)
            .collect::<String>()
            .to_lowercase(),
        size: snap_size(q.size.as_deref(), &app.cfg.render.sizes),
        theme: Theme::parse(q.theme.as_deref()),
        style: Style::parse(q.style.as_deref()),
        want_fallback: !matches!(q.fallback.as_deref(), Some("none" | "404")),
        if_none_match: headers
            .get(IF_NONE_MATCH)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
    };

    let policy = &app.cfg.policy;
    let Some(n) = normalize(raw, policy.collapse_private_suffixes, &policy.deny_suffixes) else {
        return fallback(&app, &req, FallbackReason::InvalidDomain);
    };
    let domain = app.policy.canonical(&n.registrable);
    req.seed.clone_from(&domain);
    req.domain = Some(domain.clone());
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
    size: u32,
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
        Style::Mono if meta.mono.is_empty() => {
            return fallback(app, req, FallbackReason::NoMonochrome);
        }
        Style::Mono => pick_available(&meta.mono, req.size).map(|size| Shown {
            slot: Slot::Mono,
            size,
            mono_source: meta.mono_source.map(MonoSource::as_str),
        }),
        Style::Color => meta.resolve_theme(req.theme).and_then(|t| {
            pick_available(meta.sizes(t), req.size).map(|size| Shown {
                slot: t.into(),
                size,
                mono_source: None,
            })
        }),
    };
    let Some(shown) = shown else {
        return fallback(app, req, FallbackReason::NotFound);
    };
    let etag = meta
        .etags
        .get(&object_name(shown.slot, shown.size, meta.format))
        .map(|e| format!("\"{e}\""));

    if app.cfg.cache.redirect_to_public_url
        && app.store.has_backend()
        && let Some(url) = app
            .store
            .public_url(&meta.domain, shown.slot, shown.size, meta.format)
        && let Ok(loc) = HeaderValue::from_str(&url)
    {
        inc(&app.stats.served_redirect);
        let mut r = Response::new(Body::empty());
        *r.status_mut() = StatusCode::FOUND;
        r.headers_mut().insert(LOCATION, loc);
        let out = Out {
            source,
            domain: Some(&meta.domain),
            style: req.style,
            shown: Some(shown),
            reason: None,
            stale,
        };
        decorate(&mut r, &app.cfg.http_cache.redirect, &out);
        return r;
    }

    match app
        .store
        .image(&meta.domain, shown.slot, shown.size, meta.format)
        .await
    {
        Some(data) => {
            inc(&app.stats.served_cache);
            let out = Out {
                source,
                domain: Some(&meta.domain),
                style: req.style,
                shown: Some(shown),
                reason: None,
                stale,
            };
            image(
                (data, meta.format),
                etag,
                req,
                &app.cfg.http_cache.icon,
                &out,
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
        slot: if mono { Slot::Mono } else { req.theme.into() },
        size: req.size,
        mono_source: mono.then_some("generated"),
    };
    let out = Out {
        source: Source::Generated,
        domain: req.domain.as_deref(),
        style: req.style,
        shown: Some(shown),
        reason: Some(reason),
        stale: false,
    };
    if !req.want_fallback {
        let mut r = Response::new(Body::empty());
        *r.status_mut() = StatusCode::NOT_FOUND;
        decorate(&mut r, profile, &Out { shown: None, ..out });
        return r;
    }
    let data = if reason.blocked_by().is_some() {
        app.warning_icon(req.size, req.theme, req.style)
    } else {
        app.identicon(&req.seed, req.size, req.theme, req.style)
    };
    let etag = format!("\"g-{}\"", &blake3::hash(&data).to_hex()[..20]);
    image(
        (data, app.cfg.render.format),
        Some(etag),
        req,
        profile,
        &out,
    )
}

struct Out<'a> {
    source: Source,
    domain: Option<&'a str>,
    style: Style,
    shown: Option<Shown>,
    reason: Option<FallbackReason>,
    stale: bool,
}

fn image(
    (data, format): (Bytes, OutputFormat),
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
            .insert(CONTENT_TYPE, HeaderValue::from_static(format.mime()));
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
        if shown.slot != Slot::Mono {
            h.insert(name(H_THEME), HeaderValue::from_static(shown.slot.as_str()));
        }
        h.insert(name(H_SIZE), HeaderValue::from(shown.size));
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
