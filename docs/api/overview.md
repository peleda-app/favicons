---
title: HTTP API
description: Routes, canonical URLs, statuses and CORS of the v1 contract.
---

# HTTP API

The contract is v1. `HEAD` works wherever `GET` does, and there are no query parameters.

| Route | Returns |
| ----- | ------- |
| `GET /{style}/{domain}` | The icon of a site. `style` is `color` or `mono` ([masks](../guide/monochrome.md)); anything else is `404`. `domain` is a registrable domain (eTLD+1). |
| `GET /{style}/{domain}/{shape}/{theme}` | The icon on a [badge](#badges). |
| `GET /{style}/{catalog}/{slug}` | An icon by name from a [catalog](./catalogs.md). |
| `GET /catalogs.json`, `/catalogs/{id}.json` | The [catalog lists](./catalogs.md#lists). |
| `GET /openapi.json`, `/llms.txt`, … | [Machine-readable docs](./machine-readable.md). |
| `GET /healthz`, `/metrics` | [Health and metrics](./machine-readable.md#health-and-metrics). |

Every icon body is an `image/svg+xml` written by the service. How it is made is in [Where icons come from](../guide/sources.md); the headers it carries are in [Response headers](./headers.md).

## Canonical URLs

Every icon has exactly one URL: `/{style}/{registrable domain}`. Any other form gets a permanent redirect there, without fetching anything:

| Request | Redirects to |
| ------- | ------------ |
| `/color/super-domain.evil-example.com` | `/color/evil-example.com` |
| `/color/EXAMPLE.com`, `…/example.com.` | `/color/example.com` |
| `/mono/example.com.svg` | `/mono/example.com` |
| `/color/https%3A%2F%2Fwww.example.co.uk%2Fpath` | `/color/example.co.uk` |
| `/color/example.com?anything` | `/color/example.com` |
| `/mono/example.com/` | `/mono/example.com` |

The redirect is `301` with `http_cache.canonical` headers (`immutable`, a month in browsers, a year in CDNs), so caches keep one copy per site instead of one per subdomain or URL variant. The subdomain itself is never resolved or fetched (see [Privacy](../security/privacy.md)).

Inputs that are not public domains (IP literals, `localhost`, unknown TLDs) are not redirected; they get a generated icon with reason `invalid-domain`.

## Statuses

| Status | When |
| ------ | ---- |
| `200` | Real icon (`X-Icon-Source: cache` or `origin`) or generated fallback (`X-Icon-Source: generated`). |
| `301` | Non-canonical URL. |
| `304` | `If-None-Match` matched `ETag`. |
| `404` | Unknown style or path. |
| `429` | The per-subnet rate limit was exceeded. Short `text/plain` body, `Retry-After: 1`, `Cache-Control: no-store`. |
| `503` | Load shedding or request timeout. Short `text/plain` body, `Cache-Control: no-store` (`Retry-After: 1` when shed). |

There is no key and no quota beyond the per-subnet rate limit; see [Abuse resistance](../security/abuse.md).

## Badges

`/{style}/{domain}/{shape}/{theme}`: `shape` is `rounded` or `superellipse`, `theme` is `light` or `dark`. The response carries the usual headers; the `ETag` gets a `-{shape}-{theme}` suffix. Unknown shapes or themes are `404`.

Leave out the theme (`/color/github.com/superellipse`) to follow the viewer's system scheme. The SVG then carries both themes' plate and mask colours and switches them with a `prefers-color-scheme` media query, and its `ETag` gets a `-{shape}` suffix. See [Badges and shapes](../guide/badges.md).

## CORS and timing

Any origin can call the API from JavaScript. Responses carry `Access-Control-Allow-Origin: *`, and `OPTIONS` answers a preflight with `204`, `Access-Control-Allow-Methods: GET, HEAD, OPTIONS`, `Access-Control-Allow-Headers: *` and a one-day `Access-Control-Max-Age`. No credentials are involved. `Access-Control-Expose-Headers` lists `ETag`, `Server-Timing`, `CDN-Cache-Control`, `Age`, `Retry-After` and every `X-Icon-*` header.

`Timing-Allow-Origin: *` unlocks the full Resource Timing entry for pages on other origins, and `Server-Timing: app;dur=<ms>` reports how long favicons spent on the request:

```js
const r = await fetch('https://favicons.page/color/github.com')
const [t] = performance.getEntriesByName(r.url)
console.log(t.responseStart - t.requestStart, t.serverTiming, r.headers.get('x-icon-source'))
```

A response served from a CDN cache repeats the `Server-Timing` of the request that filled it; `Age` tells the two apart.

Every response also carries `X-Content-Type-Options: nosniff`, `Content-Security-Policy: default-src 'none'; sandbox`, `Referrer-Policy: no-referrer`, `X-Frame-Options: DENY`, `Cross-Origin-Resource-Policy: cross-origin` and `X-Robots-Tag: noindex`. No cookies are ever set.
