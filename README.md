# favicons

A website icon service in Rust, built for privacy and for surviving abuse.
It fetches a site's icon once, sanitises it, stores it in your cache, and
serves it as a clean, compact **SVG**, in colour or as a monochrome mask, always
fitted edge to edge.

```
GET /v1/icon/mono/github.com
```

It is built for clients that render many icons for untrusted domains, such as
mail clients, password managers and bookmark managers. In those clients an icon
request must not leak the user, crash a server or become an attack vector.

## Features

* **Vector output.** Sites that publish SVG keep their exact shapes and
  gradients, sanitized down to bare paths. Bitmaps are traced with
  [VTracer](https://github.com/visioncortex/vtracer)'s engine (`visioncortex`)
  from the largest available image (≤256 px), resampled to 512 px and reduced
  to its dominant colours first, so even a 32 px favicon comes out smooth. Every
  icon is fitted to its box, so padded and full-bleed logos look the same size.
* **Monochrome masks.** `/v1/icon/mono/{domain}` returns a black-on-transparent shape for
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
* **Redirect to a public bucket.** With `cache.redirect_to_public_url`, cached
  icons are answered with `302` to the bucket/CDN, so the service doesn't stream bytes.
* **Browser and CDN caching.** Independent `Cache-Control` and `CDN-Cache-Control`
  profiles for icons, redirects and each kind of fallback, plus `ETag` / `304`.
* **Curated sources first.** The domain's BIMI logo, the one its owner
  publishes for mail clients, is used when present. The optional theSVG catalog
  (CC0 by default) supplies curated brand SVGs. Both are sanitized like
  everything else, and `X-Icon-Provenance` says which source won.
* **Optional upstream providers.** When a site has no usable icon, configurable
  URL templates can be tried, e.g. `https://icons.duckduckgo.com/ip3/{domain}.ico`.
  Off by default. The example services are third-party and **using them may
  violate their terms of use**; see [docs/API.md](docs/API.md#upstream-providers).
* **Palettes in the headers.** Every icon response says whether the icon brings
  its own square, rounded square or circle, its brand colour, and for light and
  dark themes a badge fill that keeps it visible, a readable text colour and a
  colour for the monochrome mask (`X-Icon-Shape`, `X-Icon-Light-Plate`, …).
  `X-Icon-Fill: bleed` marks icons with their own full background that should
  be drawn edge to edge rather than padded.
* **Deterministic fallback icons.** When there is no real icon, a symmetric
  identicon is generated from the domain. Headers say why
  (`X-Icon-Fallback-Reason: queued | not-found | blocklisted | domain-too-new | …`).
* **One URL per site.** `/v1/icon/{color|mono}/{domain}` only. Subdomains,
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
  See [docs/SECURITY.md](docs/SECURITY.md).

## Quick start

```bash
cargo run --release -- --config config.example.toml
```

```bash
curl -sI 'http://localhost:8080/v1/icon/color/github.com'
```

```
HTTP/1.1 200 OK
content-type: image/svg+xml
etag: "b154cb1768af4678af57"
cache-control: public, max-age=604800, stale-while-revalidate=86400
cdn-cache-control: public, max-age=2592000, stale-while-revalidate=604800, stale-if-error=604800
x-icon-source: cache
x-icon-domain: github.com
x-icon-style: color
x-icon-vector: original
```

Docker (a ~20 MB `scratch` image with one static binary, running as uid 65534):

```bash
docker compose up -d
```

This starts `api` and an isolated `renderer` with no network, a read-only
root, no capabilities and a seccomp-confined worker process per image. See
[Render sandbox](docs/SECURITY.md#render-sandbox). A single container also works:
images are still decoded in seccomp-confined child processes.

```bash
docker run --rm -p 8080:8080 --read-only --cap-drop ALL --security-opt no-new-privileges -e FAVICONS_CACHE__STORAGE=s3://my-bucket/icons favicons
```

## Documentation

**[peleda-app.github.io/favicons](https://peleda-app.github.io/favicons/)** is an
one-pager. Its animated demo switches real recorded icons between colour and
monochrome and tints the masks with a hue slider, and it carries the API
reference. It is a static page in [`site/`](site), deployed by the `pages`
workflow. Refresh the recorded responses from a running instance with:

```bash
./site/capture.py http://127.0.0.1:8080
```


* [HTTP API contract](docs/API.md): parameters, statuses, headers, fallback reasons, storage layout
* [Security model](docs/SECURITY.md): threats, mitigations, privacy, hardening checklist
* [config.example.toml](config.example.toml): every option with its default

Every config key can also be set via environment variable:
`FAVICONS_<SECTION>__<KEY>`, for example `FAVICONS_FETCH__WORKERS=4`.
`FAVICONS_CONFIG` points to the TOML file, `FAVICONS_LOG` sets the log filter
(`info,favicons=debug`), and `FAVICONS_LOG_FORMAT=json` switches to JSON logs.

## How a request is served

```
request ─► conn limit ─► load shed ─► subnet rate limit ─► normalize to eTLD+1
        ─► not canonical? 301 to /v1/icon/{style}/{eTLD+1} ─► blocklist
        ─► memory cache ─► storage ─► hit:  SVG (or 302 to public bucket)
                                    └► miss: fair queue ─► wait ≤ wait_ms
                                                          ├► done: SVG
                                                          └► generated icon, reason=queued
worker  ─► domain age (RDAP) ─► stream HTML until </head> ─► manifest (declared or guessed) ─► candidates
        ─► download (SSRF-safe, size-capped) ─► decode in sandboxed pool
        ─► sanitize SVG or trace bitmap (≤256 px) ─► storage + meta.json
```

## Running on a small server

The defaults suit a 1 vCPU / 512 MB box. Most of the protection comes from
keeping work off it:

1. **Put a CDN in front.** Real icons are CDN-cacheable for 30 days, so origin
   sees roughly one request per icon per month per PoP.
2. **Use object storage with a public bucket** and `redirect_to_public_url = true`.
   The service then only sends tiny `302`s, or nothing at all when the CDN caches them.
3. **Keep `fetch.workers` low** (4–8). Fetching is the expensive part; the fair
   queue makes sure honest users still get their share under a flood.
4. **Keep `render.decode_concurrency` at 1–2.** Decoding is CPU-bound and the
   limit caps the worst case of hostile images.
5. **Enable `min_domain_age_days` and a blocklist** if you serve a mail client:
   most spam domains are rejected without any outbound request.
6. **Pre-warm** the top few thousand domains after deployment by requesting them
   once. They then come from storage forever.
7. **Set `client_ip.source`** correctly behind a proxy; otherwise every request
   looks like it comes from the proxy and shares one rate-limit bucket.

## Load testing

[`loadtest/`](loadtest) holds a [k6](https://k6.io) suite. All four scenarios
run at once against a release build:

| Scenario | Rate | What it proves |
|----------|------|----------------|
| `hot_cache` | 400 rps from random subnets | cache hits stay fast (p95 < 50 ms) |
| `junk_flood` | 300 rps of IPs, `localhost`, unknown TLDs, blocklisted hosts | junk is answered without any outbound request |
| `attacker` | 600 rps of unique domains from one `/24` | the subnet gets rate-limited (> 50% `429`), no 5xx |
| `honest_during_attack` | 50 rps from other subnets | > 99% of honest requests still get the real icon |

```bash
./loadtest/run.sh -e DURATION=30s -e SCALE=1
```

It uses a local `k6` if installed, otherwise the `grafana/k6` Docker image. The
`loadtest` workflow runs it weekly, on pushes to `main` and on demand, and
writes the summary to the job page.

## Performance

The same k6 suite on two very different machines. `SCALE=1` is ~1 400 rps
across the four scenarios, `SCALE=2` doubles every rate, and so on. Every run
kept **100%** of honest requests getting the real icon, had no 5xx, and
rate-limited the attacking subnet. CPU is the favicons process alone.

**A weak, busy box:** `homeassistant-relay`, an Intel Celeron N3160 thin
client (4 × 1.6 GHz, 3.5 GB RAM, 8 GB eMMC). It keeps running a Chromium wall
panel, a Zigbee serial bridge and a BLE gateway, so favicons ran at low CPU
priority with a 600 MB memory cap. Load came from a laptop on the LAN through
an SSH tunnel, which costs this box more CPU than favicons itself.

| Load | Throughput | p95 | p99 | favicons CPU | Memory peak |
|------|-----------:|----:|----:|-------------:|------------:|
| `SCALE=1` | 1 406 rps | 6.8 ms | 12.9 ms | 0.52 of 4 cores | 73 MB |
| `SCALE=2` | 2 818 rps | 7.8 ms | 10.4 ms | 0.80 of 4 cores | 73 MB |

The other services there never noticed: the wall panel, Zigbee and BLE stayed
up through every run.

**A laptop:** Apple M3 Pro (12 cores, 36 GB), with k6 in Docker on the same
machine. At `SCALE=8` k6 itself starts falling behind, so that row is a floor,
not a ceiling.

| Load | Throughput | p95 | p99 | favicons CPU | Memory peak |
|------|-----------:|----:|----:|-------------:|------------:|
| `SCALE=1` | 1 410 rps | 2.2 ms | 4.6 ms | 0.35 cores | 49 MB |
| `SCALE=2` | 2 821 rps | 2.0 ms | 4.2 ms | 0.34 cores | 72 MB |
| `SCALE=4` | 5 640 rps | 1.7 ms | 3.9 ms | 0.50 cores | 87 MB |
| `SCALE=8` | 11 277 rps | 1.2 ms | 3.5 ms | 0.47 cores | 98 MB |

Part of the CPU is fixed background work (fetching what the rate-limited
attacker gets through), which is why the cost per request falls as load rises.
These are cache hits and junk; fetching a new site's icon is bounded by
`fetch.workers` and the network, not by these numbers.

## Development

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## License

[MIT](LICENSE)
