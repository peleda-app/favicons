# Self-hosting

## Run it

```bash
cargo run --release -- --config config.example.toml
```

```bash
curl -sI 'http://localhost:8080/color/github.com'
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

Docker: `favicons/favicons` (also `ghcr.io/peleda-app/favicons`) is the API alone, a `scratch` image for
amd64 and arm64 with one static binary, running as uid 65534 (8.5 MB to
download). `ghcr.io/peleda-app/favicons.page` is what runs favicons.page: the page at `/`,
its fonts and every catalog prerendered. Tags: `latest` and `main` follow the main branch, `sha-<commit>`
pins a build, and `1.2.3` / `1.2` appear for release tags.

Each [release](https://github.com/peleda-app/favicons/releases) also carries the
static binaries as `favicons-<version>-<arch>-linux-musl` and
`favicons-site-<version>-<arch>-linux-musl` (x86_64 and aarch64) with a
`SHA256SUMS` file. `cargo install favicons` builds the API from crates.io; the
page needs a checkout and `cargo build --release --features site`.

```bash
docker compose up -d
```

This starts `api` and an isolated `renderer` with no network, a read-only
root, no capabilities and a seccomp-confined worker process per image. See
[Render sandbox](SECURITY.md#render-sandbox). A single container also works:
images are still decoded in seccomp-confined child processes.

```bash
docker run --rm -p 8080:8080 --read-only --cap-drop ALL --security-opt no-new-privileges -e FAVICONS_CACHE__STORAGE=s3://my-bucket/icons favicons/favicons
```

## Configuration

[config.example.toml](../config.example.toml) lists every option with its default.
Every key can also be set via environment variable:
`FAVICONS_<SECTION>__<KEY>`, for example `FAVICONS_FETCH__WORKERS=4`.
`FAVICONS_CONFIG` points to the TOML file, `FAVICONS_LOG` sets the log filter
(`info,favicons=debug`), and `FAVICONS_LOG_FORMAT=json` switches to JSON logs.

The page at `/` is compiled into the binary; `server.landing = false` turns it
off and `server.public_url` names it. It is only compiled into builds with the
`site` feature (`cargo build --features site`, the `favicons-site` release binary, or the `favicons.page` image).

## HTTPS without a proxy

favicons can terminate TLS itself and get its certificate from Let's Encrypt:

```toml
[tls]
enabled = true
domains = ["favicon.example.com"]
contact = ["mailto:ops@example.com"]
```

It listens on `:443` and `:80`. Port 80 answers `http-01` challenges and
redirects everything else to HTTPS. Until the first certificate arrives it
serves a self-signed one; the account, key and certificate are kept in
`tls.state_dir`, so restarts reuse them instead of hitting Let's Encrypt rate
limits, and renewal runs in the background 30 days before expiry. Set
`tls.cert` and `tls.key` to use a fixed certificate instead (for example a
Cloudflare Origin CA certificate).

Behind Cloudflare with **Full (strict)**, the first certificate needs the
challenge to reach the origin over plain HTTP: if **Always Use HTTPS** is on,
add a rule that turns it off for `/.well-known/acme-challenge/*`. Once a valid
certificate exists, renewals work through either port.

## Running on a small server

The defaults suit a 1 vCPU / 512 MB box. Most of the protection comes from
keeping work off it:

1. **Put a CDN in front.** Real icons are CDN-cacheable for 30 days, so origin
   sees roughly one request per icon per month per PoP.
2. **Cap the cache.** Object storage (S3, GCS, Azure) keeps it off the box; set
   lifecycle rules on the bucket. On a local disk, cap it with `cache.max_disk_bytes` or
   `cache.max_disk_entries`: the least recently used domains are deleted first.
   The cap costs no memory per icon; a background pass walks the directory
   when usage crosses it, at most once a minute, and otherwise once an hour.
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
