# Security model

favicons takes an attacker-controlled string, makes outbound requests to
attacker-controlled hosts, parses attacker-controlled HTML and images, and
serves the result to third parties (often inside e-mail clients). Every one of
those steps is treated as hostile.

## Threats and mitigations

### 0. DNS

All lookups go through a built-in [hickory](https://github.com/hickory-dns/hickory-dns)
resolver, not the OS resolver. The default is **Cloudflare "Security" over
DNS-over-HTTPS** (`security.cloudflare-dns.com`, 1.1.1.2 / 1.0.0.2):

* Queries are encrypted and authenticated (TLS with the server name pinned), so
  neither the network nor the hosting provider can see or spoof which domains are
  looked up. Spoofed answers can't redirect the fetcher.
* Cloudflare answers `0.0.0.0` for domains it classifies as malware or phishing.
  The worker resolves the domain before fetching anything. A sinkholed domain is
  stored with status `blocked` and served as a red warning icon with reason
  `dns-blocked` and `X-Icon-Blocked: dns-filter`, and the site is never contacted.
* Quad9 (`provider = "quad9"`) also blocks malware; `cloudflare` and `google` don't
  filter; `custom` takes any DoH/DoT/UDP server; `system` uses the OS config.
* Trade-off: the filter judges the name it is asked about. Because requests are
  collapsed to the registrable domain for privacy, a malicious **subdomain** on an
  otherwise clean domain (`malware.example.com`) is not caught by DNS. Resolving
  the original subdomain would let its owner see the lookup and defeat the
  read-receipt protection, so it is deliberately not done. Blocklists still match
  the full requested host without resolving it.
* The resolver runs entirely on the async runtime (no `getaddrinfo` blocking
  threads) and caches answers (`dns.cache_size`, capped at `dns.max_ttl_secs`).
* Cloudflare/Quad9 see the queried domains, though not which user asked: queries
  come from the service and are shared and cached across all users.

### 1. SSRF: reaching internal services through the fetcher

| Attack | Mitigation |
|--------|------------|
| `?domain=127.0.0.1`, `[::1]`, `2130706433`, `0x7f.1` | IP literals are rejected at input parsing; only DNS names with a known public suffix pass. |
| DNS name resolving to `10.0.0.5`, `169.254.169.254`, `fd00::1`, … | Custom resolver drops every non-global address (RFC 1918, loopback, link-local, CGNAT, ULA, multicast, documentation, benchmarking, 6to4/NAT64/Teredo that embed private IPv4). If nothing public remains the connection is never opened. |
| DNS rebinding (public on check, private on connect) | The filtered address list from the resolver **is** the connect target; there is no second lookup. |
| Redirect to `http://127.0.0.1/`, `https://203.0.113.7/` or `https://internal:6379/` | Redirects are never followed automatically. Each `Location` is validated before the next request: https only, port 443 only, no userinfo and no IP-address hosts by default (`fetch.allow_http`, `fetch.allowed_ports`, `fetch.allow_ip_hosts`; even with IP hosts allowed, only public addresses pass). Its host then passes the full [foreign-host check](#8-assets-and-redirects-on-other-domains). At most `fetch.max_redirects` hops. |
| Icon `<link href>` / manifest `src` pointing inside, at an IP address or at another port | The same URL rules are sent into the sandboxed parser, so such URLs never even become candidates. |
| Environment proxies (`HTTP_PROXY`) | Disabled (`no_proxy`). |

### 2. Resource exhaustion through fetched content

* HTML is streamed only until `</head>` or `<body>` appears (128 KiB cap). The
  connection is then dropped, so a multi-megabyte page costs a few KiB. Icon links
  on popular sites sit in the first ~25 KiB even when `<head>` itself is huge.
* Hard byte caps: manifest 64 KiB, icon 1 MiB. They are enforced while streaming
  and count bytes **after** HTTP `Content-Encoding` decompression, so a gzip/brotli
  bomb stops at the cap with bounded memory.
* Guessed manifest paths are tried only when the host answered at all, the
  page declared no manifest and no icon ≥128 px was found. There are at most 3
  probes, and anything that doesn't start with `{` (e.g. an SPA's HTML catch-all)
  is discarded before JSON parsing.
* Connect, request, DNS and whole-job timeouts.
* At most `fetch.max_candidates` icon downloads per domain, `fetch.workers`
  concurrent jobs in total.

### 3. Malicious images (decoder exploits, bombs, polyglots)

* All decoders are memory-safe Rust (`image`, `png`, `zune-jpeg`, `image-webp`,
  `gif`, `resvg`/`usvg`, `visioncortex` for tracing). No libpng, libwebp,
  ImageMagick, Potrace or librsvg. The crate
  itself is built with `unsafe_code = "forbid"`.
* Format is chosen by magic bytes, never by `Content-Type`. Only PNG, ICO,
  JPEG, GIF (first frame), WebP, BMP and SVG are accepted. TIFF, AVIF, HEIC and
  everything else is rejected.
* **Decompression bombs.** A 2 KiB PNG can claim 60000×60000 px (14 GB of RGBA).
  Before any pixel data is inflated, the header is read and the declared size is
  checked against `render.max_source_dimension` (1024). `image::Limits` enforces the
  same bound plus a 32 MiB allocation cap inside every decoder (PNG, ICO entries,
  GIF frames, WebP, JPEG, BMP). Tests cover PNG, PNG-in-ICO, WebP and GIF bombs,
  and each must be rejected in under a second.
* Generic compressed containers (gzip, zlib, bzip2, xz, zstd, lz4, zip) are
  refused outright. This also closes `svgz`, which `usvg` would otherwise
  inflate without a limit.
* Aspect ratio above 4:1 is rejected.
* **Every image is decoded in a fresh, sandboxed process** (see
  [Render sandbox](#render-sandbox)). A crash, hang or exploit in a decoder
  dies with that process.
* At most `render.decode_concurrency` workers run at once, each with a
  `render.decode_timeout_ms` deadline after which it is killed.
* **Nothing is ever passed through.** Every icon is rebuilt as a new SVG, either
  re-serialized from the parsed vector or traced from decoded pixels. Metadata
  chunks, trailing payloads, polyglot HTML/JS and embedded scripts don't survive.

### 4. Malicious SVG

SVG is the riskiest format, so it gets extra rules:

* ≤ 256 KiB, `<!ENTITY` declarations rejected (billion laughs), markup depth
  ≤ 48 (stack exhaustion).
* `usvg` is built without text and system fonts. External references
  (`<image href>`, file paths, `http:`, `data:`) resolve to nothing: an SVG
  can't read local files or make network requests.
* The output SVG is written by an **allowlist serializer**, not by copying or
  filtering the input. It can only emit `<svg>`, `<defs>`, `<path>` (`fill`,
  `fill-opacity`, `fill-rule`, `d`), `<linearGradient>`, `<radialGradient>` and
  `<stop>`, with numeric values and `#rrggbb` colours. Scripts, `<style>` and CSS
  (including `@media` and `@import`), `<foreignObject>`, `<image>` and base64
  payloads, links, event handlers, animations, text, filters, masks and external
  references have no way to be expressed in it.
* Everything the serializer drops is detected. If the re-rendered result differs
  from the original by more than `render.svg_fidelity`, the icon is traced from
  pixels instead of shipping a half-broken vector.
* **Second check outside the sandbox.** The parent re-parses every SVG the worker
  returns with a strict scanner: element and attribute allowlists per nesting
  level, value character classes, `url(#gN)` only, no comments, entities, text
  nodes or extra roots, and a size cap. A compromised worker can't smuggle markup
  through. Only after that check does the parent add a hash suffix to the
  gradient ids (`g0-941ba8`).
* Badges are composed by the server around that checked SVG. The only CSS in
  them is the server's own `<style>` with the plate and mask colours and a
  `prefers-color-scheme` media query, built from validated `#rrggbb` values.
* Set `render.svg = false` to disable SVG completely.

### 5. Responses abused against users

* Always `Content-Type: image/svg+xml` + `X-Content-Type-Options: nosniff`.
* `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'none'; sandbox`:
  even if a browser opens the SVG as a document, nothing can execute or load.
  Inline styles stay allowed so plates without a theme can follow the system
  scheme; with `default-src 'none'` CSS can't fetch anything.
* No cookies, no `Set-Cookie`, `Referrer-Policy: no-referrer`.
* `Access-Control-Allow-Origin: *` and `Timing-Allow-Origin: *` without
  credentials, so pages can draw the icon on a canvas and measure the request
  without leaking anything. `Server-Timing` carries only the request duration.

### 6. Abuse of the service itself (DoS)

Defense in depth, cheapest first:

1. **Connection layer.** `server.max_connections` cap (excess sockets are closed
   immediately), HTTP/1 header read timeout (slowloris), 32 KiB header buffer,
   HTTP/2 stream and header-list limits.
2. **Load shedding.** `server.max_concurrent_requests` in flight; beyond that
   `503` is returned instantly instead of queueing.
3. **Per-subnet token bucket.** Client IPs are reduced to `/24` (IPv4) or `/56`
   (IPv6), then hashed (see below). Over the limit: `429`, no work done. The
   limiter table is a bounded LRU, so rotating through IPv6 space can't exhaust memory.
4. **The request path never waits on the network for long.** Cache hits come from
   memory or storage. Misses wait at most `fetch.wait_ms` and then get a generated
   icon with `X-Icon-Fallback-Reason: queued`.
5. **Fair fetch queue.** Each subnet has its own bucket in the queue, and workers
   serve buckets round-robin. A client flooding a million unique domains gets the
   same share of fetch throughput as a client asking for one. Each bucket is capped
   (`fetch.per_client_queue_capacity`). When the global queue is full, the **largest**
   bucket loses its newest entry; if the newcomer is already the largest, the
   newcomer is rejected (`overloaded`).
6. **Deduplication.** Concurrent requests for the same domain share a single fetch.
7. **Negative caching.** Failures and missing icons are cached (`*_ttl_secs`), and
   storage misses are cached in memory for `cache.absent_ttl_secs`, so repeat floods
   for junk domains don't reach origin or storage.
8. **Generated icons are cached** in a bounded in-memory LRU.

### 7. Malicious or throw-away domains

* **Blocklists** (`policy.blocklists`, local files or URLs, hosts/adblock/plain
  formats, reloaded periodically). A match on the requested host or any parent
  returns a generated icon and **never contacts the site**.
* **Domain age** (`policy.min_domain_age_days`, default 30 days). The registration date comes from
  the registry's RDAP server, found via the IANA bootstrap file (no third-party
  aggregator). Domains younger than the threshold are never fetched. This blocks
  the classic "register a fresh domain per campaign" pattern.
* **Filtering DNS** (see [DNS](#0-dns)). Domains sinkholed by the resolver are
  never fetched.
* **Allowlist** (`policy.allowlist`) overrides blocklists and domain age.

Suggested lists: abuse.ch URLhaus / ThreatFox host lists, Phishing Army, OISD.
Choose by licence and update frequency; they are not bundled.

### 8. Assets and redirects on other domains

Sites often serve icons from a CDN (`github.com` → `github.githubassets.com`,
`spotify.com` → `open.spotifycdn.com`). A page could just as well point at a
malicious domain registered yesterday. Every host the service is about to
contact for an icon, manifest or redirect hop passes the same checks as the
requested domain, before the request is made:

1. Valid public DNS name with a known suffix (no IP literals, `localhost`, unknown TLDs).
2. Blocklists: the host and every parent domain.
3. Domain age via RDAP (`policy.min_domain_age_days`, default 30). This applies
   to hosts outside the requested registrable domain; RDAP answers are cached
   for a day.
4. Filtering DNS: a sinkholed host never gets a connection, because the
   resolver refuses it.
5. SSRF rules: public addresses only, https on port 443, no IP-address hosts
   (all three configurable in `[fetch]`).

A rejected host is logged (`host rejected … reason=…`) and the next candidate
is tried, usually the site's own `/apple-touch-icon.png` or `/favicon.ico`.

## Privacy

The main client of this service is a mail client rendering sender avatars.
The service must not become a read-receipt or tracking oracle.

| Leak | Mitigation |
|------|------------|
| Recipient's IP reaching the sender | Users never contact the sender's server; the service fetches on their behalf. |
| Per-recipient subdomains (`u-48213.spam.tld`) used as a read receipt | Input is collapsed to the **registrable domain** (eTLD+1) using the Public Suffix List. Subdomains are never resolved or fetched. Private PSL sections (`*.github.io`, `*.workers.dev`, …) are collapsed too (`policy.collapse_private_suffixes`), so tenant subdomains on shared platforms don't leak. |
| One fresh registrable domain per recipient | Domain-age gate plus blocklists. The fetch is also shared across all users and cached for `cache.ttl_secs`, so at most one request per domain per TTL ever reaches the sender. |
| Timing correlation ("the icon was fetched 2 s after I sent the mail") | Queue + shared cache + `stale-while-revalidate` mean origin fetches aren't tied to a specific user request. Run behind a CDN to decouple further. |
| BIMI and catalog | BIMI is a TXT lookup through the DoH resolver plus a download from the host the record names, which passes every foreign-host check. The theSVG index and icons come from GitHub (`raw.githubusercontent.com`) and reveal only which brand slug is being fetched. |
| Upstream providers | Off by default. When enabled, the registrable domain (never the user, never the subdomain) is sent to the configured third-party service, and only for sites that have no usable icon of their own. The result is cached like any other icon. |
| Our own logs | Raw client IPs are never logged or stored. Rate limiting uses `blake3_keyed(random_per_process_key, subnet)`. The key never leaves memory and changes on restart, so even a memory dump can't be joined with other data across restarts. |
| Referer / cookies to origins | Outbound requests send no `Referer`, no cookies and a fixed User-Agent. |

## Render sandbox

Parsing attacker-controlled bytes is the most likely place for a memory-safety
bug to be found, even in Rust parsers. So none of it runs inside the service
that has network access, credentials and the cache: images, HTML `<head>` and
web app manifests are all handled by the sandboxed worker. The parent re-checks
everything the worker returns: response format, sizes, URL safety and inline
data limits. A compromised worker therefore can't inject internal URLs.

**Worker process (`render.sandbox = "process"`, default).** For every image, page
or manifest the
service starts `favicons render-worker`: same binary, empty environment, cwd `/`,
stderr discarded. The worker first reads the request from stdin, then locks
itself down before touching a single image byte:

| Layer | Setting |
|-------|---------|
| `RLIMIT_AS` | `render.worker_memory_bytes` (256 MiB) |
| `RLIMIT_CPU` | `decode_timeout_ms` rounded up; the kernel kills runaway loops |
| `RLIMIT_NOFILE` | 0: no new file descriptors of any kind |
| `RLIMIT_NPROC` | 0: no new processes or threads |
| `RLIMIT_CORE` | 0: no core dumps with image data |
| `PR_SET_NO_NEW_PRIVS` | no privilege gain through any exec |
| seccomp-bpf | allowlist of ~20 syscalls (memory, futex, write to stdout, exit). Anything else, such as `open`, `socket`, `connect`, `execve`, `ptrace` or `fork`, **kills the process** with `SIGSYS` |

The parent kills the worker on timeout (`kill_on_drop`), accepts at most 8 MiB
of output, and validates the response format, sizes and variant count before
using it. A seccomp kill is logged as such.

Seccomp is Linux-only. On macOS (development) the worker is isolated only by
the process boundary and rlimits, and the service warns at startup.

**Separate renderer container (`render.sandbox = "socket"`).** `docker-compose.yml`
runs the same image twice:

* `api`: network access, talks to the renderer over a unix socket on a shared
  volume.
* `renderer`: `favicons render-server` with `network_mode: none` (no network
  interface except loopback), read-only root, `cap_drop: ALL`,
  `no-new-privileges`, uid 65534, `pids_limit: 32`, memory and CPU caps. The
  socket is `0600`. It still starts a fresh seccomp worker per image, and it
  replaces the limits sent by `api` with its own config, so a compromised `api`
  can't loosen them.

**Image.** `FROM scratch` with a single statically linked (musl) binary: no shell,
no libc, no package manager. Fetching icons uses root certificates compiled into
the binary; the Mozilla CA bundle in `/etc/ssl/certs` is there only for the
S3, GCS and Azure clients, which read it from disk. About 22 MB unpacked,
7.3 MB to download. Published as `ghcr.io/peleda-app/favicons` for amd64 and
arm64.

**Verify on your host.** Each probe must be denied; on Linux it is killed with
exit code 159 (128 + SIGSYS):

```bash
for p in fs write net exec prctl; do docker run --rm favicons sandbox-selftest $p; echo "$p: $?"; done
```

`sandbox-selftest render` must succeed (exit 0). CI runs the same checks.

## Operational hardening (recommended)

* Run as non-root with a read-only root filesystem (see `Dockerfile`), and
  drop all capabilities.
* Put a CDN in front and let it absorb traffic: `CDN-Cache-Control` defaults to
  30 days for real icons.
* Egress firewall: allow only TCP 80/443 outbound plus DNS. This is a second
  line of defence for SSRF even if a bug slips through.
* Expose `/metrics` only on `server.admin_listen` bound to a private interface.
* Set `client_ip.source` only when a trusted proxy sets that header; otherwise
  clients can spoof their subnet.

## Possible future hardening

* User namespaces / gVisor (`runsc`) for the renderer container.
* Proof-of-work or signed URLs for anonymous high-volume clients.

## Verifying builds

Every published image and release binary carries a signed SLSA build
provenance attestation (Sigstore, keyless, issued to this repository's GitHub
Actions workflow). It proves the artifact was built by `.github/workflows/docker.yml`
from a specific commit of this repository:

```bash
gh attestation verify oci://ghcr.io/peleda-app/favicons:latest --repo peleda-app/favicons
gh attestation verify favicons-1.3.0-x86_64-linux-musl --repo peleda-app/favicons
```

The output names the commit and workflow run. Images can be pinned by the
verified digest.

## Reporting a vulnerability

Please use GitHub's private vulnerability reporting on this repository rather
than a public issue.
