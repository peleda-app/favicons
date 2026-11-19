# HTTP API contract (v1)

## `GET /v1/icon/{domain}`

`HEAD` is also supported.

`{domain}` may be a bare host (`mail.example.com`), a host with a `.webp` or `.png` suffix
(`example.com.webp`) or a URL-encoded URL (`https%3A%2F%2Fexample.com%2Fpath`).
It is always reduced to a **registrable domain** (eTLD+1) before anything else
happens. See [Privacy](SECURITY.md#privacy).

### Query parameters

| Name       | Values                     | Default     | Meaning |
|------------|----------------------------|-------------|---------|
| `size`     | integer                    | `32`        | Snapped down to the largest size enabled in `render.sizes` (default `16`, `32`, `64`, `128`). Values below the smallest enabled size become that size. |
| `theme`    | `light`, `dark`            | `light`     | Preferred variant. When the site has no variant for this theme, the other one is served and `X-Icon-Theme` says which. |
| `style`    | `color`, `mono`            | `color`     | `mono` returns a [monochrome mask](#monochrome-icons). `theme` is ignored for `mono`. |
| `fallback` | `generated`, `none`, `404` | `generated` | `generated` always returns an image. `none` / `404` return `404` with an empty body and the same headers, so clients can draw their own placeholder. |

### Monochrome icons

`style=mono` returns a single-colour version: **black (`#000`) on transparent,
with the shape entirely in the alpha channel**. It is meant for CSS masks, so
the consumer picks the colour:

```css
.site-icon {
  width: 16px;
  height: 16px;
  background-color: currentColor;
  -webkit-mask: url("https://icons.example.com/v1/icon/github.com?style=mono&size=32") center / contain no-repeat;
  mask: url("https://icons.example.com/v1/icon/github.com?style=mono&size=32") center / contain no-repeat;
}
```

`X-Icon-Mono-Source` says where the shape came from, best first:

| Value | Meaning |
|-------|---------|
| `mask-icon` | The site's own monochrome asset: `<link rel="mask-icon">` (Safari pinned tab SVG) or a manifest icon with `purpose: monochrome`. Only its alpha channel is used, so the declared `color` doesn't matter. |
| `derived` | Converted from the site's colour icon. A logo on an opaque tile becomes the logo without the tile. A logo on a flat "plate" (white circle, coloured disc) keeps the plate with the details knocked out. A logo on transparency becomes its silhouette. |
| `generated` | A monochrome version of the generated fallback (identicon pattern, or the `!` of the blocked-domain warning). Comes with `X-Icon-Fallback`. |

When the site has a colour icon but no shape could be derived from it (too
little contrast), the response is a generated mono icon with reason
`no-monochrome`.

### Size fallback

The service never upscales by more than 10%. When the source is smaller than the
requested size, the largest available size **not above** the request is served.
When even 16 px is not available, the smallest stored size is served.
`X-Icon-Size` always contains the real pixel size of the body.

### Responses

| Status | When |
|--------|------|
| `200`  | Real icon (`X-Icon-Source: cache` or `origin`) or generated fallback (`X-Icon-Source: generated`). Body is `image/webp` (lossless) or `image/png` depending on `render.format`, always re-encoded by the service. |
| `302`  | `cache.redirect_to_public_url = true` and the icon exists in storage. `Location` points to `{cache.public_base_url}/v1/{domain}/{theme}-{size}.{webp|png}`. |
| `304`  | `If-None-Match` matched `ETag`. |
| `404`  | `fallback=none` and no real icon is available. |
| `429`  | The per-subnet rate limit was exceeded. Short `text/plain` body, `Retry-After: 1`, `Cache-Control: no-store`. |
| `503`  | Load shedding or request timeout. Short `text/plain` body, `Cache-Control: no-store` (`Retry-After: 1` when shed). |

### Response headers

| Header | Values | Present |
|--------|--------|---------|
| `Content-Type` | `image/webp` or `image/png` | 200 |
| `ETag` | `"<hash>"` for real icons, `"g-<hash>"` for generated ones | 200, 304 |
| `Cache-Control` | Browser directive from the matching `http_cache.*` profile | always |
| `CDN-Cache-Control` | CDN directive from the matching `http_cache.*` profile ([RFC 9213](https://www.rfc-editor.org/rfc/rfc9213)) | 200, 302, 304, 404 |
| `X-Icon-Source` | `cache`, `origin`, `generated` | 200, 302, 304, 404 |
| `X-Icon-Domain` | Canonical registrable domain after aliasing, e.g. `microsoft.com` for `outlook.live.com` | when the input was valid |
| `X-Icon-Style` | `color`, `mono` | always on icon responses |
| `X-Icon-Theme` | `light`, `dark` | 200, 302, 304 for `style=color` |
| `X-Icon-Mono-Source` | `mask-icon`, `derived`, `generated` | `style=mono` |
| `X-Icon-Size` | `16`, `32`, `64`, `128` | 200, 302, 304 |
| `X-Icon-Fallback` | `true` | when a generated icon (or 404) is returned |
| `X-Icon-Fallback-Reason` | see below | when `X-Icon-Fallback` is present |
| `X-Icon-Stale` | `true` | the cached entry is past its TTL; a refresh was queued |
| `X-Icon-Blocked` | `blocklist`, `dns-filter` | the domain is blocked; tells which mechanism blocked it |

Every response also carries `X-Content-Type-Options: nosniff`,
`Content-Security-Policy: default-src 'none'; sandbox`, `Referrer-Policy: no-referrer`,
`X-Frame-Options: DENY`, `Cross-Origin-Resource-Policy: cross-origin`,
`Access-Control-Allow-Origin: *` and `X-Robots-Tag: noindex`. No cookies are ever set.

### Fallback reasons

| Reason | Cache profile | Meaning |
|--------|---------------|---------|
| `queued` | `fallback_transient` | The domain is not cached yet. A fetch is queued; retry later. |
| `overloaded` | `fallback_transient` | The fetch queue is full and this client already has more queued work than others. |
| `fetch-failed` | `fallback_transient` | The site was unreachable (DNS, TLS, timeouts, private IPs). Retried after `cache.fetch_failed_ttl_secs`. |
| `not-found` | `fallback_stable` | The site answered but exposes no usable icon. Retried after `cache.not_found_ttl_secs`. |
| `domain-too-new` | `fallback_stable` | RDAP registration date is younger than `policy.min_domain_age_days`. |
| `blocklisted` | `fallback_stable` | The host or one of its parents is on a configured blocklist. `X-Icon-Blocked: blocklist`. |
| `dns-blocked` | `fallback_stable` | The filtering DNS resolver sinkholed the domain (answered `0.0.0.0`). `X-Icon-Blocked: dns-filter`. |
| `no-monochrome` | `fallback_stable` | `style=mono` was requested, the site has a colour icon, but neither a mask icon nor a usable derived shape exists. |
| `invalid-domain` | `fallback_stable` | Not a public, registrable DNS name (IP literals, `localhost`, unknown TLDs, `.onion`, …). |

Blocked domains (`blocklisted`, `dns-blocked`) get a fixed **warning icon**: a red
rounded square with a white exclamation mark, the same for every domain, in a
slightly brighter red for `theme=dark`. Clients can match on `X-Icon-Blocked`
to show their own warning UI.

All other generated icons are deterministic: `blake3(identicon.salt || 0x00 || canonical_domain)`
seeds a symmetric 5×5 pattern, so the same domain always gets the same image
on every instance that shares the salt.

### Caching guidance for clients

* Use the query string as part of the cache key; there is no `Vary`.
* `fallback_transient` responses are short-lived by design. A client that shows
  a `queued` icon gets the real one on its next refresh.
* Honour `ETag` / `If-None-Match`; revalidation is cheap.

## `GET /healthz`

`200 ok`. Available on the public and the admin listener.

## `GET /metrics` (admin listener only)

Prometheus text format. Enable with `server.admin_listen`. Never expose it publicly.

| Metric | Type |
|--------|------|
| `favicons_requests_total` | counter |
| `favicons_served_cache_total`, `favicons_served_redirect_total` | counter |
| `favicons_rate_limited_total`, `favicons_shed_total` | counter |
| `favicons_fetch_{ok,not_found,failed,too_new}_total` | counter |
| `favicons_decode_errors_total` | counter |
| `favicons_fallback_total{reason}` | counter |
| `favicons_queue_length`, `favicons_fetch_running` | gauge |
| `favicons_blocklist_entries`, `favicons_alias_entries` | gauge |

## Storage layout

```
{prefix}/v1/{domain}/meta.json
{prefix}/v1/{domain}/{light|dark|mono}-{16|32|64|128}.{webp|png}
```

`meta.json`:

```json
{
  "domain": "github.com",
  "fetched_at": 1795132800,
  "status": "ok",
  "format": "webp",
  "light": [16, 32, 64, 128],
  "dark": [],
  "mono": [16, 32, 64, 128],
  "mono_source": "mask-icon",
  "etags": { "light-16.webp": "…", "light-32.webp": "…" }
}
```

`status` is one of `ok`, `not-found`, `fetch-failed`, `too-new`, `blocked`. `format` records
the encoding the objects were written in, so changing `render.format` doesn't
break existing entries; they switch format on their next refresh. Image objects are
written with their `Content-Type` and `Cache-Control` from `http_cache.icon.browser`
(S3/GCS/Azure only), so a public bucket behind a CDN serves them correctly.
