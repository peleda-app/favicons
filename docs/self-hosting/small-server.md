---
title: Small servers
description: Running favicons on a 1 vCPU, 512 MB box.
---

# Small servers

The defaults suit a 1 vCPU / 512 MB box. Most of the protection comes from keeping work off it:

1. **Put a CDN in front.** Real icons are CDN-cacheable for 30 days, so origin sees roughly one request per icon per month per PoP.
2. **Cap the cache.** Object storage keeps it off the box; on a local disk use `cache.max_disk_bytes` or `cache.max_disk_entries` ([storage](./storage.md)).
3. **Keep `fetch.workers` low** (4–8). Fetching is the expensive part; the fair queue keeps honest users served under a flood.
4. **Keep `render.decode_concurrency` at 1–2.** Decoding is CPU-bound and the limit caps the worst case of hostile images.
5. **Enable `min_domain_age_days` and a blocklist** if you serve a mail client: most spam domains are rejected without any outbound request.
6. **Pre-warm** the top few thousand domains after deployment by requesting them once.
7. **Set `client_ip.source`** correctly behind a proxy ([details](./tls.md#behind-a-proxy)).

Measured throughput is in [Benchmarks](../performance/benchmarks.md).
