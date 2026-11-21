# How favicons works

* **Vector output.** Sites that publish SVG keep their exact shapes and
  gradients, sanitized down to bare paths. Bitmaps are traced with
  [VTracer](https://github.com/visioncortex/vtracer)'s engine (`visioncortex`)
  from the largest available image (≤256 px), resampled to 512 px and reduced
  to its dominant colours first, so even a 32 px favicon comes out smooth. Every
  icon is fitted to its box, so padded and full-bleed logos look the same size.
* **Monochrome masks.** `/mono/{domain}` returns a black-on-transparent shape for
  CSS `mask-image`, so consumers can tint it any colour. It is taken from the
  site's `mask-icon` or manifest `monochrome` icon when available, otherwise
  derived from the colour icon.
* **Smart discovery.** `<link rel=icon|apple-touch-icon>`, web app manifests,
  inline `data:` icons, `/apple-touch-icon.png`, `/favicon.ico`. When the page
  declares no manifest and offers nothing ≥128 px (or HTML is blocked by bot
  protection), `/manifest.json`, `/site.webmanifest` and `/manifest.webmanifest`
  are tried. The largest usable candidate wins. Icons on CDNs or other domains
  are used only after that host passes the same blocklist, domain-age, DNS-filter
  and SSRF checks as the requested domain, and so does every redirect hop.
  By default only `https` on port 443 is used, and links or redirects to raw IP
  addresses are ignored (`fetch.allowed_ports`, `fetch.allow_ip_hosts`).
* **Pluggable cache.** `none` (memory only), local directory, S3 and S3-compatible
  stores, GCS, Azure. TTLs are separate for real icons, missing icons, failures
  and too-new domains.
* **Browser and CDN caching.** Independent `Cache-Control` and `CDN-Cache-Control`
  profiles for icons, redirects and each kind of fallback, plus `ETag` / `304`.
* **Curated sources first.** The domain's BIMI logo, the one its owner
  publishes for mail clients, is used when present. The optional theSVG catalog
  (CC0 by default) supplies curated brand SVGs. Both are sanitized like
  everything else, and `X-Icon-Provenance` says which source won.
* **Optional upstream providers.** When a site has no usable icon, configurable
  URL templates can be tried, e.g. `https://icons.duckduckgo.com/ip3/{domain}.ico`.
  Off by default. The example services are third-party and **using them may
  violate their terms of use**; see [docs/API.md](API.md#upstream-providers).
* **Palettes in the headers.** Every icon response says whether the icon brings
  its own square, rounded square or circle, its brand colour, and for light and
  dark themes a badge fill that keeps it visible, a readable text colour and a
  colour for the monochrome mask (`X-Icon-Shape`, `X-Icon-Light-Plate`, …).
  `X-Icon-Fill: bleed` marks icons with their own full background that should
  be drawn edge to edge rather than padded.
* **Deterministic fallback icons.** When there is no real icon, a symmetric
  identicon is generated from the domain. Headers say why
  (`X-Icon-Fallback-Reason: queued | not-found | blocklisted | domain-too-new | …`).
* **One URL per site.** `/{color|mono}/{domain}` only. Subdomains,
  case variants, query strings and trailing slashes get a cacheable `301` to
  the registrable domain, so caches never fill up with copies of one icon.
* **Privacy.** Every request is collapsed to its registrable domain
  (`u-123.tracker.example.com` → `example.com`), so per-recipient subdomains
  can't be used as read receipts. Raw client IPs are never logged.
* **Abuse resistance.** Per-subnet rate limiting, a fair round-robin fetch queue
  that evicts the heaviest client first, load shedding, negative caching,
  blocklists and a domain-age gate via RDAP.
* **Encrypted, filtering DNS.** By default every lookup goes over DNS-over-HTTPS
  to Cloudflare's malware-blocking resolver (1.1.1.2); sinkholed domains are
  never fetched. Quad9, other providers, DoT, custom servers or the system
  resolver are one config line away.
* **Sandboxed rendering.** Each image is decoded in a throw-away process locked
  down with seccomp (no files, no network, no exec) and rlimits, optionally in a
  separate container with no network at all.
* **Defensive rendering.** Memory-safe decoders only, strict size, pixel and
  time limits, SVG without file or network access, and every output rebuilt as
  a fresh SVG by an allowlist serializer. Only the HTML `<head>` is downloaded, and declared
  image dimensions are checked before any decompression, so bombs never inflate.
  See [docs/SECURITY.md](SECURITY.md).

## How a request is served

```
request ─► conn limit ─► load shed ─► subnet rate limit ─► normalize to eTLD+1
        ─► not canonical? 301 to /{style}/{eTLD+1} ─► blocklist
        ─► memory cache ─► storage ─► hit:  SVG
                                    └► miss: fair queue ─► wait ≤ wait_ms
                                                          ├► done: SVG
                                                          └► generated icon, reason=queued
worker  ─► domain age (RDAP) ─► stream HTML until </head> ─► manifest (declared or guessed) ─► candidates
        ─► download (SSRF-safe, size-capped) ─► decode in sandboxed pool
        ─► sanitize SVG or trace bitmap (≤256 px) ─► storage + meta.json
```
