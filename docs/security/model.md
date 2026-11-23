---
title: Security model
description: What is treated as hostile, DNS, SSRF and malicious domains.
---

# Security model

favicons takes an attacker-controlled string, makes outbound requests to attacker-controlled hosts, parses attacker-controlled HTML and images, and serves the result to third parties, often inside e-mail clients. Every one of those steps is treated as hostile.

| Threat | Page |
| ------ | ---- |
| DNS spoofing, SSRF, throw-away and foreign domains | this page |
| Decoder exploits, bombs, malicious SVG, responses abused against users | [Images and SVG](./images.md) |
| Denial of service | [Abuse resistance](./abuse.md) |
| Code execution in parsers | [Render sandbox](./sandbox.md) |
| Read receipts and tracking | [Privacy](./privacy.md) |
| Deployment and supply chain | [Hardening](./hardening.md) |

## DNS

All lookups go through a built-in [hickory](https://github.com/hickory-dns/hickory-dns) resolver, not the OS resolver. The default is **Cloudflare "Security" over DNS-over-HTTPS** (`security.cloudflare-dns.com`, 1.1.1.2 / 1.0.0.2):

- Queries are encrypted and authenticated (TLS with the server name pinned), so neither the network nor the hosting provider can see or spoof which domains are looked up.
- Cloudflare answers `0.0.0.0` for domains it classifies as malware or phishing. The worker resolves the domain before fetching anything. A sinkholed domain is stored as `blocked` and served as a red warning icon with reason `dns-blocked` and `X-Icon-Blocked: dns-filter`; the site is never contacted.
- Quad9 (`provider = "quad9"`) also blocks malware; `cloudflare` and `google` don't filter; `custom` takes any DoH/DoT/UDP server; `system` uses the OS config.
- Trade-off: because requests are collapsed to the registrable domain for privacy, a malicious **subdomain** on a clean domain is not caught by DNS. Resolving it would let its owner see the lookup and defeat the read-receipt protection. Blocklists still match the full requested host without resolving it.
- The resolver runs on the async runtime (no blocking `getaddrinfo` threads) and caches answers (`dns.cache_size`, capped at `dns.max_ttl_secs`).
- The resolver sees the queried domains, though not which user asked: queries come from the service and are shared and cached across all users.

## SSRF

| Attack | Mitigation |
| ------ | ---------- |
| `127.0.0.1`, `[::1]`, `2130706433`, `0x7f.1` | IP literals are rejected at input parsing; only DNS names with a known public suffix pass. |
| A name resolving to `10.0.0.5`, `169.254.169.254`, `fd00::1`, … | The resolver drops every non-global address (RFC 1918, loopback, link-local, CGNAT, ULA, multicast, documentation, benchmarking, 6to4/NAT64/Teredo embedding private IPv4). If nothing public remains the connection is never opened. |
| DNS rebinding | The filtered address list **is** the connect target; there is no second lookup. |
| Redirect to `http://127.0.0.1/`, an IP host or another port | Redirects are never followed automatically. Each `Location` is validated: https only, port 443 only, no userinfo and no IP hosts by default (`fetch.allow_http`, `fetch.allowed_ports`, `fetch.allow_ip_hosts`; even with IP hosts allowed, only public addresses pass). Its host then passes the full [foreign-host check](#assets-and-redirects-on-other-domains). At most `fetch.max_redirects` hops. |
| Icon `href` or manifest `src` pointing inside | The same URL rules are applied in the sandboxed parser, so such URLs never become candidates. |
| Environment proxies (`HTTP_PROXY`) | Disabled. |

## Resource exhaustion

- HTML is streamed only until `</head>` or `<body>` appears (128 KiB cap), then the connection is dropped.
- Hard byte caps: manifest 64 KiB, icon 1 MiB, counted **after** `Content-Encoding` decompression, so a gzip or brotli bomb stops at the cap.
- Guessed manifest paths are tried only when the host answered, declared no manifest and offered no icon ≥128 px: at most 3 probes, and anything not starting with `{` is discarded before JSON parsing.
- Connect, request, DNS and whole-job timeouts; at most `fetch.max_candidates` downloads per domain and `fetch.workers` concurrent jobs.

## Malicious or throw-away domains

- **Blocklists** (`policy.blocklists`). A match on the requested host or any parent returns a generated icon and **never contacts the site**.
- **Domain age** (`policy.min_domain_age_days`, default 30). The registration date comes from the registry's RDAP server via the IANA bootstrap file, with no third-party aggregator. This blocks the "fresh domain per campaign" pattern.
- **Filtering DNS** (above).
- **Allowlist** (`policy.allowlist`) overrides blocklists and domain age.

## Assets and redirects on other domains

Sites often serve icons from a CDN (`github.com` → `github.githubassets.com`). A page could just as well point at a domain registered yesterday. Every host the service is about to contact for an icon, manifest or redirect hop passes the same checks as the requested domain first:

1. A valid public DNS name with a known suffix.
2. Blocklists: the host and every parent domain.
3. Domain age via RDAP, for hosts outside the requested registrable domain; answers are cached for a day.
4. Filtering DNS.
5. SSRF rules: public addresses only, https on port 443, no IP hosts.

A rejected host is logged (`host rejected … reason=…`) and the next candidate is tried, usually the site's own `/apple-touch-icon.png` or `/favicon.ico`.
