---
title: Response headers
description: Every header an icon response can carry.
---

# Response headers

| Header | Values | Present |
| ------ | ------ | ------- |
| `Content-Type` | `image/svg+xml` | 200 |
| `ETag` | `"<hash>"` for real icons, `"g-<hash>"` for generated ones | 200, 304 |
| `Cache-Control` | Browser directive from the matching `http_cache.*` profile | always |
| `CDN-Cache-Control` | CDN directive from the matching profile ([RFC 9213](https://www.rfc-editor.org/rfc/rfc9213)) | 200, 304, 404 |
| `X-Icon-Source` | `cache`, `origin`, `generated` | 200, 304, 404 |
| `X-Icon-Domain` | Registrable domain that was served | when the input was valid |
| `X-Icon-Style` | `color`, `mono` | icon responses |
| `X-Icon-Mono-Source` | `mask-icon`, `derived`, `generated` ([masks](../guide/monochrome.md)) | `mono` |
| `X-Icon-Vector` | `original`, `traced`, `generated` ([vector output](../guide/sources.md#vector-output)) | 200, 304 |
| `X-Icon-Provenance` | `bimi`, `thesvg`, `site`, `upstream`, `catalog` | real icons |
| `X-Icon-License` | SPDX id, e.g. `CC0-1.0` | catalog icons |
| `X-Icon-Upstream` | host of the upstream provider | icons from an [upstream provider](../self-hosting/configuration.md#upstream-providers) |
| `X-Icon-Fallback` | `true` | a generated icon (or 404) |
| `X-Icon-Fallback-Reason` | see [Fallbacks](./fallbacks.md) | with `X-Icon-Fallback` |
| `X-Icon-Stale` | `true` | the cached entry is past its TTL; a refresh was queued |
| `X-Icon-Blocked` | `blocklist`, `dns-filter` | the domain is blocked |
| `X-Icon-Shape`, `X-Icon-Fill`, `X-Icon-Accent`, `X-Icon-{Light,Dark}-{Plate,Text,Mono,Mono-Plate}` | see [Palette headers](./palette.md) | 200, 304 |

CORS, timing and security headers are described in [CORS and timing](./overview.md#cors-and-timing).
