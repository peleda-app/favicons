# favicons

A website icon service in Rust, built for privacy and for surviving abuse.
It fetches a site's icon once, sanitises it, stores it in your cache, and
serves it as a clean WebP (or PNG) in 16, 32, 64 or 128 px, light or dark.

```
GET /v1/icon/github.com?size=64&theme=dark
```

It is built for clients that render many icons for untrusted domains, such as
mail clients, password managers and bookmark managers. In those clients an icon
request must not leak the user, crash a server or become an attack vector.

## Features

* **Sizes and themes.** `size=16|32|64|128` and `theme=light|dark`
  (`<link media="(prefers-color-scheme: dark)">` is understood). A missing size
  falls back to the largest smaller one; a missing theme falls back to the other theme.
* **Monochrome masks.** `style=mono` returns a black-on-transparent shape for
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
* **Pluggable cache.** `none` (memory only), local directory, S3 and S3-compatible
  stores, GCS, Azure. TTLs are separate for real icons, missing icons, failures
  and too-new domains.
* **Redirect to a public bucket.** With `cache.redirect_to_public_url`, cached
  icons are answered with `302` to the bucket/CDN, so the service doesn't stream bytes.
* **Browser and CDN caching.** Independent `Cache-Control` and `CDN-Cache-Control`
  profiles for icons, redirects and each kind of fallback, plus `ETag` / `304`.
* **Deterministic fallback icons.** When there is no real icon, a symmetric
  identicon is generated from the domain. Headers say why
  (`X-Icon-Fallback-Reason: queued | not-found | blocklisted | domain-too-new | …`).
* **Domain aliases.** Vaultwarden/Bitwarden equivalent-domain lists, so
  `outlook.live.com`, `office.com` and `bing.com` all get the Microsoft icon.
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
  time limits, SVG without file or network access, and every output re-encoded
  to lossless WebP or PNG. Only the HTML `<head>` is downloaded, and declared
  image dimensions are checked before any decompression, so bombs never inflate.
  See [docs/SECURITY.md](docs/SECURITY.md).

## Quick start

```bash
cargo run --release -- --config config.example.toml
```

```bash
curl -sI 'http://localhost:8080/v1/icon/github.com?size=64'
```

```
HTTP/1.1 200 OK
content-type: image/webp
etag: "b154cb1768af4678af57"
cache-control: public, max-age=604800, stale-while-revalidate=86400
cdn-cache-control: public, max-age=2592000, stale-while-revalidate=604800, stale-if-error=604800
x-icon-source: cache
x-icon-domain: github.com
x-icon-theme: light
x-icon-size: 64
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
interactive one-pager. It has ten presets replaying real API responses, an
animated colour picker that tints monochrome masks with live CSS, and the API
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
        ─► alias (vaultwarden) ─► blocklist
        ─► memory cache ─► storage ─► hit:  WebP (or 302 to public bucket)
                                    └► miss: fair queue ─► wait ≤ wait_ms
                                                          ├► done: WebP
                                                          └► generated icon, reason=queued
worker  ─► domain age (RDAP) ─► stream HTML until </head> ─► manifest (declared or guessed) ─► candidates
        ─► download (SSRF-safe, size-capped) ─► decode in sandboxed pool
        ─► square + resize ─► re-encode WebP ─► storage + meta.json
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
writes the summary to the job page. On a laptop the suite sustains ~1 200 rps
with p99 ≈ 6 ms.

## Development

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## License

[MIT](LICENSE)
