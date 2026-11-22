# HTTP API contract (v1)

## `GET /{style}/{domain}`

`HEAD` is also supported. There are no query parameters.

| Segment | Values | Meaning |
|---------|--------|---------|
| `style` | `color`, `mono` | `mono` returns a [monochrome mask](#monochrome-icons). Anything else is `404`. |
| `domain` | a registrable domain (eTLD+1) | The site whose icon you want. |

### Where icons come from

Sources are tried in this order, and the first usable one wins:

1. **BIMI** (`bimi.enabled`, on by default). The domain's
   `default._bimi.<domain>` TXT record names the logo its owner publishes for
   mail clients, often backed by a Verified Mark Certificate. The service does
   not validate the certificate; the record itself is the owner's statement.
2. **theSVG catalog** (`catalog.thesvg`, off by default). Curated brand SVGs.
   The index is synced daily, and an entry matches only when its `url` is the
   site's root. Only the configured licenses are used (`CC0-1.0` by default),
   and the license is returned in `X-Icon-License`. The catalog's `mono` variant
   becomes the monochrome icon.
3. **The site itself**: `<link rel=icon>`, manifests, `mask-icon`, `/favicon.ico`.
4. **Upstream providers**, if configured (see below).

When BIMI or the catalog delivers an icon, the site's own pages are not fetched
at all. Every source, curated or not, is untrusted input: it gets the same host
checks, sandboxed parsing and allowlist serializer.

Brand logos are trademarks regardless of the license of the SVG file. Showing a
site's own mark next to its address is the usual use, but check whether your use
case is covered.

### Upstream providers

Optional and off by default. When the site itself offers no usable icon, or
cannot be reached, the URL templates in `upstream.providers` are tried in order,
with `{domain}` replaced by the registrable domain:

```toml
[upstream]
providers = [
  "https://favicon.vemetric.com/{domain}?size=256&format=webp",
  "https://icons.duckduckgo.com/ip3/{domain}.ico",
  "https://www.google.com/s2/favicons?domain={domain}&sz=64",
]
```

> **Examples only.** These URLs are only examples of the template format. They are third-party
services with their own terms of use, which may not allow this kind of access.
Check their terms before enabling one, or point this at a service you run
yourself.

Upstream responses get the same treatment as any foreign host: https only, SSRF
rules, blocklists, domain age, DNS filter, checked redirects, size caps. A
non-2xx answer (several services answer `404` for unknown domains) moves on to
the next provider. The image is vectorized in the sandbox like any other bitmap,
and the response carries `X-Icon-Upstream`. The monochrome variant is derived
from it as usual.

Providers are never asked about a domain that does not resolve. Once a day each
provider is asked about a random domain that does not exist; an answer
identical to that placeholder is skipped as if it were a `404`.

### Loops

Every request the fetcher makes carries `X-Favicons-Fetch: 1`. A request with
that header gets an icon only if one is already cached; anything else is a
`404` with no fallback image and nothing queued. A site whose `<link rel=icon>`
points back at this server, or at another favicons instance, therefore can
neither recurse nor end up with a generated icon as its own.

### Canonical URLs

Every icon has exactly one URL: `/{style}/{registrable domain}`. Any
other form gets a permanent redirect there, without fetching anything:

| Request | Redirects to |
|---------|--------------|
| `/color/super-domain.evil-example.com` | `/color/evil-example.com` |
| `/color/EXAMPLE.com`, `…/example.com.` | `/color/example.com` |
| `/mono/example.com.svg` | `/mono/example.com` |
| `/color/https%3A%2F%2Fwww.example.co.uk%2Fpath` | `/color/example.co.uk` |
| `/color/example.com?anything` | `/color/example.com` |
| `/mono/example.com/` | `/mono/example.com` |

The redirect is `301` with `http_cache.canonical` headers (`immutable`, a month in
browsers, a year in CDNs), so browsers and CDNs remember it and keep one cached
copy per site instead of one per subdomain or URL variant. The subdomain itself is
never resolved or fetched (see [Privacy](SECURITY.md#privacy)).

Inputs that are not public domains (IP literals, `localhost`, unknown TLDs) are
not redirected; they get a generated icon with reason `invalid-domain`.

### Monochrome icons

`/mono/{domain}` returns a single-colour SVG: **black (`#000`) paths on
transparency**. It is meant for CSS masks, so
the consumer picks the colour. When a derived mask has parts in clearly
different colours, such as a logo's letters, a ribbon's folds or a mascot's
eyes, they come back as two or three stacked layers with `fill-opacity`, so the
mask shows them as lighter shades of the one tint. A site's own `mask-icon` is
kept as a single shape.

```css
.site-icon {
  width: 16px;
  height: 16px;
  background-color: currentColor;
  -webkit-mask: url("https://icons.example.com/mono/github.com") center / contain no-repeat;
  mask: url("https://icons.example.com/mono/github.com") center / contain no-repeat;
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

### Vector output

Every response body is an SVG produced by the service, never the site's own file:

| `X-Icon-Vector` | Meaning |
|-----------------|---------|
| `original` | The site published an SVG (`rel=icon type=image/svg+xml`, `mask-icon`, manifest). It was parsed in the sandbox and **re-serialized by an allowlist writer**: only `<path>` with solid fills, linear and radial gradients survive. Strokes become outlines and transforms are baked into coordinates. The result is rendered and compared with the original; if too much was lost (patterns, masks, clip paths), the icon is traced instead. |
| `traced` | Traced from the largest bitmap the site offers (capped at `render.trace_max_px`, 256 px). The bitmap is resampled to 512 px and reduced to a palette of at most 12 dominant colours, with anti-aliasing blends dropped, then traced into stacked spline paths. Shapes of one colour become one path where z-order allows. Smooth bitmap gradients become a few flat bands. |
| `generated` | Identicon or blocked-domain warning, drawn as vector. |

Every SVG is `128×128` (`viewBox="0 0 128 128"`) and scales to any size on the
client. Its content is **fitted to the box**: scaled until it touches the edges
on its longer side, then centred. A full-size plain white backdrop counts as
padding and is removed first, so a logo on a white app tile ends up as large as a
logo without padding. Coloured tiles are part of the design and kept. Output is
capped at `render.max_output_svg_bytes` (48 KiB).

### Responses

| Status | When |
|--------|------|
| `200`  | Real icon (`X-Icon-Source: cache` or `origin`) or generated fallback (`X-Icon-Source: generated`). Body is always `image/svg+xml`, generated by the service. |
| `301`  | Non-canonical URL; see [Canonical URLs](#canonical-urls). |
| `304`  | `If-None-Match` matched `ETag`. |
| `404`  | Unknown style or path. |
| `429`  | The per-subnet rate limit was exceeded. Short `text/plain` body, `Retry-After: 1`, `Cache-Control: no-store`. |
| `503`  | Load shedding or request timeout. Short `text/plain` body, `Cache-Control: no-store` (`Retry-After: 1` when shed). |

### Response headers

| Header | Values | Present |
|--------|--------|---------|
| `Content-Type` | `image/svg+xml` | 200 |
| `ETag` | `"<hash>"` for real icons, `"g-<hash>"` for generated ones | 200, 304 |
| `Cache-Control` | Browser directive from the matching `http_cache.*` profile | always |
| `CDN-Cache-Control` | CDN directive from the matching `http_cache.*` profile ([RFC 9213](https://www.rfc-editor.org/rfc/rfc9213)) | 200, 304, 404 |
| `X-Icon-Source` | `cache`, `origin`, `generated` | 200, 304, 404 |
| `X-Icon-Domain` | Registrable domain that was served | when the input was valid |
| `X-Icon-Style` | `color`, `mono` | always on icon responses |
| `X-Icon-Mono-Source` | `mask-icon`, `derived`, `generated` | `mono` |
| `X-Icon-Vector` | `original`, `traced`, `generated` | 200, 304 |
| `X-Icon-Provenance` | `bimi`, `thesvg`, `site`, `upstream` | real icons: where the shape came from |
| `X-Icon-License` | SPDX id, e.g. `CC0-1.0` | icons from the theSVG catalog |
| `X-Icon-Upstream` | host of the upstream provider, e.g. `icons.duckduckgo.com` | when the icon came from [an upstream provider](#upstream-providers) |
| `X-Icon-Fallback` | `true` | when a generated icon (or 404) is returned |
| `X-Icon-Fallback-Reason` | see below | when `X-Icon-Fallback` is present |
| `X-Icon-Stale` | `true` | the cached entry is past its TTL; a refresh was queued |
| `X-Icon-Blocked` | `blocklist`, `dns-filter` | the domain is blocked; tells which mechanism blocked it |
| `X-Icon-Shape`, `X-Icon-Fill`, `X-Icon-Accent`, `X-Icon-{Light,Dark}-{Plate,Text,Mono,Mono-Plate}` | see [Palette headers](#palette-headers) | 200, 304, on real and generated icons |

### CORS and timing

Any origin can call the API from JavaScript. Responses carry
`Access-Control-Allow-Origin: *`, and `OPTIONS` answers a preflight with `204`,
`Access-Control-Allow-Methods: GET, HEAD, OPTIONS`, `Access-Control-Allow-Headers: *`
and a one-day `Access-Control-Max-Age`. No credentials are involved.
`Access-Control-Expose-Headers` lists `ETag`, `Server-Timing`,
`CDN-Cache-Control`, `Age`, `Retry-After` and every `X-Icon-*` header, so
`fetch()` on another origin can read them.

`Timing-Allow-Origin: *` unlocks the full Resource Timing entry (DNS, connect,
TLS, time to first byte) for pages on other origins, and
`Server-Timing: app;dur=<ms>` reports how long favicons itself spent on the
request:

```js
const url = "https://favicons.page/color/github.com";
const r = await fetch(url);
const [t] = performance.getEntriesByName(r.url);
console.log(t.responseStart - t.requestStart, t.serverTiming, r.headers.get("x-icon-source"));
```

A response served from a CDN cache repeats the `Server-Timing` of the request
that filled it; `Age` tells the two apart.

Every response also carries `X-Content-Type-Options: nosniff`,
`Content-Security-Policy: default-src 'none'; sandbox`, `Referrer-Policy: no-referrer`,
`X-Frame-Options: DENY`, `Cross-Origin-Resource-Policy: cross-origin`,
`Access-Control-Allow-Origin: *`, `Timing-Allow-Origin: *` and `X-Robots-Tag: noindex`. No cookies are ever set.

### Palette headers

Colours to put around the icon, so it stays visible on any theme. Both styles
carry the same set, computed from the colour icon:

```
x-icon-shape: circle
x-icon-fill: inset
x-icon-accent: #1ed760
x-icon-light-plate: #1c1c1e
x-icon-light-text: #13883d
x-icon-light-mono: #17a249
x-icon-light-mono-plate: #e7f8ed
x-icon-dark-plate: #183f26
x-icon-dark-text: #1ed760
x-icon-dark-mono: #1ed760
x-icon-dark-mono-plate: #1d442b
```

| Header | Meaning |
|--------|---------|
| `X-Icon-Shape` | Whether the colour icon brings its own background: `square`, `rounded` (square with rounded corners), `circle`, or `none` for a free-standing logo. Read from the outline, so a square with a transparent glyph cut out of it is still `square`. |
| `X-Icon-Fill` | `bleed`: the icon brings its own background right up to the edges (a square, or a rounded square with a corner radius up to 25% of its side). Draw it at 100% of your badge and clip it to your own corners; padding would leave a small tile floating on a plate. `inset`: a free-standing logo, a circle or a very round tile; draw it with padding on the plate. |
| `X-Icon-Accent` | The icon's dominant brand colour; the darkest neutral for black-and-white logos. |
| `X-Icon-Light-*`, `X-Icon-Dark-*` | One scheme per theme. The light theme assumes a `#ffffff` background, the dark one `#1c1c1e`. |
| `…-Plate` | Fill for a badge behind the colour icon. Prefers a soft tint of the accent, then neutral, then the opposite brightness. For a free-standing logo it must contrast with most of the logo; for a tile with holes it must contrast with the tile itself, so a black square with a transparent glyph never gets a black plate and turns into a plain black square. |
| `…-Text` | The accent, darkened or lightened to at least 4.5:1 against the theme background, for a name or link next to the icon. Neutral logos get the theme's regular light text on dark. |
| `…-Mono` | Colour for the `mono` mask, at least 3:1 (the WCAG minimum for graphics) on its mono plate and therefore on the theme background too. |
| `…-Mono-Plate` | Badge fill for the `mono` mask: a soft tint of the same hue close to the theme background, so badge and icon read as one colour. |

Every icon is fitted edge to edge. Inside a badge, give `inset` icons the same
padding (around 18% of the side works well) and draw `bleed` icons at full size.

The palette is computed once per fetch, in the sandbox, from a 64 px rendering
of the final colour SVG, and stored in `meta.json`. An entry cached before
palettes existed is served without them and refreshed in the background.

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
| `no-monochrome` | `fallback_stable` | `mono` was requested, the site has a colour icon, but neither a mask icon nor a usable derived shape exists. |
| `invalid-domain` | `fallback_stable` | Not a public, registrable DNS name (IP literals, `localhost`, unknown TLDs, `.onion`, …). |
| `not-cached` | `no-store` | A [private request](#private-requests) for a domain that is not cached. Nothing was fetched or queued. |

Blocked domains (`blocklisted`, `dns-blocked`) get a fixed **warning icon**: a red
square (a red gradient with a warm highlight) and a white exclamation mark, the
same for every domain. Clients can match on `X-Icon-Blocked`
to show their own warning UI.

All other generated icons are deterministic: `blake3(identicon.salt || 0x00 || canonical_domain)`
seeds a symmetric 5×5 white pattern on a layered background: a linear gradient
plus three soft radial glows. The seed picks a harmonious hue scheme (hues stay
within about 100° of each other, so overlaps never turn muddy), the gradient
direction, and the position, size and colour of every glow. Lightness is capped
on yellow and green hues so the white pattern stays readable. The same domain
always gets the same image on every instance that shares the salt.

### Private requests

A private request reads the cache and nothing else. The service makes no
outbound connection for it: no fetch, no queue entry, no background refresh of a
stale icon, no DNS, RDAP or BIMI lookup, no upstream provider and no catalog
download. Nobody outside the instance learns that the domain was looked up.

* A cached icon is served as usual, stale or not.
* Anything else gets the generated icon with `X-Icon-Fallback-Reason:
  not-cached` and `Cache-Control: no-store` plus `CDN-Cache-Control: no-store`,
  so a CDN never hands that placeholder to ordinary requests.
* Plates, palettes and masks are computed locally from the cached icon and work
  the same way.

Two ways to ask for it:

* **A private host.** Requests whose `Host` is listed in
  `server.private_hosts` are private. On favicons.page that is
  `https://private.favicons.page/color/github.com`. It works in a plain
  `<img src>` and keeps its own CDN cache.
* **A header.** `X-Icon-Private: 1` on any request, for `fetch()` and servers.
  Cross-origin browser requests with it go through a CORS preflight, which the
  service allows.

The popular sites and the icon catalogs prerendered into the favicons.page
image are always cached, so they are always available privately.

### Caching guidance for clients

* Use the query string as part of the cache key; there is no `Vary`.
* `fallback_transient` responses live for 10 seconds in browsers and CDNs, so
  a flood or a cold queue is never cached for long. On `queued` or
  `overloaded`, retry after a short pause with `cache: "reload"`.
* Honour `ETag` / `If-None-Match`; revalidation is cheap.
* Stored icons are sent gzipped to clients that send `Accept-Encoding: gzip`,
  with `Vary: Accept-Encoding` and their own `ETag` (suffix `-gz`). Generated
  fallbacks are never compressed. `http_cache.compression = "off"` sends raw SVG.

## `GET /{style}/{domain}/{shape}/{theme}`

The same icon, already drawn on its badge: `shape` is `rounded` (22.5% corner
radius) or `superellipse` (n = 5, like iOS app icons), `theme` is `light` or
`dark`. The plate takes the theme's `…-Plate` colour, `inset` icons get 18%
padding and `bleed` icons fill the shape and are clipped to it. With `mono` the
mask is tinted with `…-Mono` on the `…-Mono-Plate` colour. Catalog icons work the
same way: `/color/selfhst/jellyfin/superellipse/dark`.

```html
<img src="https://favicons.page/color/github.com/superellipse/light" width="40" height="40" alt="">
```

The response carries the usual headers; the `ETag` gets a `-{shape}-{theme}`
suffix. Unknown shapes or themes are `404`.

Leave out the theme to follow the viewer's system scheme:
`/color/github.com/superellipse`, `/mono/selfhst/jellyfin/rounded`. The SVG then
carries both themes' plate and mask colours and switches them with a
`prefers-color-scheme` media query, so one URL works on light and dark pages and
flips with the operating system. Its `ETag` gets a `-{shape}` suffix. Apps
with their own theme switch can swap between the `/light` and `/dark` URLs
instead.

Every SVG is safe to paste inline: gradient ids look like `g0-941ba8` and the
badge's classes and ids like `.p-bf9106`, with a hash of the icon, so icons in
one page never share or override each other's styles and gradients.

```html
<img src="https://favicons.page/color/github.com/superellipse" width="40" height="40" alt="">
```

## `GET /{style}/{catalog}/{slug}`

Optional catalogs (`[[catalogs]]` in the config) serve icons by name rather
than by domain, e.g. self-hosted apps from [selfh.st](https://selfh.st/icons/):
`/color/selfhst/jellyfin`, `/mono/selfhst/jellyfin`. The response is the same
as for a domain: an SVG, the palette headers, `ETag` and cache profiles, with
`X-Icon-Provenance: catalog`, `X-Icon-License` (the icon's own license when the
index has one, like theSVG, else the catalog's) and `X-Icon-Domain: @{catalog}.{slug}`.
The `mono` style uses the catalog's black variant when it has one, otherwise it
is derived from the colour icon.

Supported index formats:

| `format` | Catalog | Notes |
|----------|---------|-------|
| `selfhst` | [selfh.st icons](https://selfh.st/icons/), CC BY 4.0 | SVG, PNG when there is no SVG, black `-dark` variant for `mono` |
| `simple-icons` | [Simple Icons](https://simpleicons.org), CC0 | single-colour marks, tinted with each brand's colour for `color` |
| `dashboard-icons` | [Dashboard Icons](https://github.com/homarr-labs/dashboard-icons), Apache-2.0 | SVG or PNG; `mono` is derived |
| `devicon` | [Devicon](https://devicon.dev), MIT | the `original` variant, else `plain`, `wordmark` or `line` |
| `thesvg` | [theSVG](https://github.com/GLINCKER/thesvg), per icon | every collection; `licenses` keeps only the listed licenses |
| `iconify` | any [Iconify JSON](https://github.com/iconify/icon-sets) set, e.g. Fluent Emoji (MIT), Twemoji (CC BY 4.0), Noto Emoji (Apache-2.0) | SVGs come from the index itself; `mono_index_url` points at a second set with the same names for `mono`, e.g. Fluent Emoji High Contrast |

Logos stay trademarks of their owners whatever the file's license; show them to
identify the brand, not to imply endorsement.

| Status | When |
|--------|------|
| `200` | The icon, or a generated one with `X-Icon-Fallback-Reason: queued` on the first request. |
| `301` | Upper case, a `.svg` suffix or a query string; `Location` is the canonical path. |
| `404` | Unknown style, catalog or slug. Nothing is fetched for a slug the index does not list. |
| `503` | The catalog index has not loaded yet (`Retry-After: 5`). |

`GET /catalogs.json` lists the configured catalogs (`id`, `name`, `license`,
`attribution`, `count`). `GET /catalogs/{id}.json` adds `items`: every `slug`
with its `name`, and `category` and `tags` when the index has them.

## The page and machine-readable docs

All compiled into the binary. Their names and absolute URLs come from
`server.public_url`, else `https://{tls.domains[0]}`, else the request's `Host`
header (validated, never cached).

| Path | What |
|------|------|
| `/` | The demo page. It calls `/color` and `/mono` on the same origin, loads nothing from other hosts, and allows only its own inline script by hash in its `Content-Security-Policy`. |
| `/favicon.svg`, `/apple-touch-icon.png`, `/og.png` | Icons and the 1200×630 share image. |
| `/sitemap.xml` | The page's URL. |
| `/robots.txt` | Lets search engines index the page but not `/color/` and `/mono/`, and lets user-driven AI agents (`ChatGPT-User`, `Claude-User`, `Perplexity-User`, `MistralAI-User`) fetch everything. Icon responses also carry `X-Robots-Tag: noindex`. |
| `/llms.txt` | How to use the API, for language models and agents ([llmstxt.org](https://llmstxt.org)). |
| `/openapi.json` | OpenAPI 3.1 description, generated at start from this server's config: its origin, version, configured catalogs (as an enum) and whether the page is built in. Builds without the page also serve it at `/`. |

The page, its images and its fonts (Archivo and Martian Mono, OFL) are only in
builds with the `site` feature, such as the `favicons.page` image; it loads
nothing from other hosts.

`server.landing = false` turns off the page, the images and the sitemap;
`robots.txt`, `llms.txt` and `openapi.json` stay.

## `GET /healthz`

`200 ok`. Available on the public and the admin listener.

## `GET /metrics` (admin listener only)

Prometheus text format. Enable with `server.admin_listen`. Never expose it publicly.

| Metric | Type |
|--------|------|
| `favicons_requests_total` | counter |
| `favicons_served_cache_total`, `favicons_canonical_redirects_total` | counter |
| `favicons_rate_limited_total`, `favicons_shed_total` | counter |
| `favicons_fetch_{ok,not_found,failed,too_new}_total` | counter |
| `favicons_decode_errors_total` | counter |
| `favicons_fallback_total{reason}` | counter |
| `favicons_queue_length`, `favicons_fetch_running` | gauge |
| `favicons_blocklist_entries` | gauge |

## Storage layout

```
{prefix}/v4/{domain}/meta.json
{prefix}/v4/{domain}/{color|mono}.svg
```

A new layout version starts with an empty cache. On a local directory the
server removes the folders of older layouts (`v3`, …) in the background on
start; object stores keep them until you delete them.

`meta.json`:

```json
{
  "domain": "github.com",
  "fetched_at": 1795132800,
  "status": "ok",
  "color": "original",
  "mono": "original",
  "mono_source": "mask-icon",
  "etags": { "color.svg": "…", "mono.svg": "…" }
}
```

`status` is one of `ok`, `not-found`, `fetch-failed`, `too-new`, `blocked`. Each
slot holds the vector origin (`original` / `traced`) or `null`. Entries
prerendered into the `cache.seed` directory (a tree like this one, or `.pack`
files written by `favicons pack`) carry `"pinned": true`: they are
served as they are, never queued for a refresh, and win over a copy of the same
domain in the working cache. The service always
serves icons itself; storage is never exposed to clients.
