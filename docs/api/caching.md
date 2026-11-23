---
title: Caching
description: Browser and CDN cache profiles, ETags and compression.
---

# Caching

Every response carries `Cache-Control` for browsers and `CDN-Cache-Control` for CDNs from one of the `http_cache.*` profiles: icons, canonical redirects, `fallback_transient`, `fallback_stable` and errors. Real icons stay in CDNs for 30 days by default.

- Use the full URL as the cache key; there is no `Vary` except for compression.
- `fallback_transient` responses live for 10 seconds, so a flood or a cold queue is never cached for long. On `queued` or `overloaded`, retry after a short pause with `cache: "reload"`.
- Honour `ETag` / `If-None-Match`; revalidation is cheap.
- Stored icons are sent gzipped to clients that send `Accept-Encoding: gzip`, with `Vary: Accept-Encoding` and their own `ETag` (suffix `-gz`). Generated fallbacks are never compressed. `http_cache.compression = "off"` sends raw SVG.
- A stale entry is served with `X-Icon-Stale: true` while a refresh runs in the background.

Server-side cache backends and TTLs are in [Storage and cache](../self-hosting/storage.md).
