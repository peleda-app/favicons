---
title: Introduction
description: What favicons does and what one icon URL gives you.
---

# Introduction

favicons turns any website's icon into a clean, safe SVG and tells you which colours to put around it on light and dark themes. There is one URL per site, no key, no query parameters and open CORS.

```html
<img src="https://favicons.page/color/github.com" width="32" height="32" alt="" />
```

## What you get

- **Vector output.** Sites that publish SVG keep their exact shapes and gradients. Bitmaps are traced into smooth paths, so even a 32 px `favicon.ico` scales to any size. See [Where icons come from](./sources.md).
- **Colours for both themes.** Every response carries a badge fill, a readable text colour and a mask tint for light and dark. See [Themes and palettes](./themes.md).
- **Ready-made badges.** Rounded and superellipse badges that follow the viewer's colour scheme. See [Badges and shapes](./badges.md).
- **Monochrome masks.** Single-colour shapes for CSS `mask-image`. See [Monochrome masks](./monochrome.md).
- **Icons by name.** About 17 000 self-hosted apps and brands from open catalogs. See [Catalogs](./catalogs.md).
- **Private and hardened.** Requests collapse to the registrable domain, every image is decoded in a sandbox and rebuilt by an allowlist writer. See [Security model](../security/model.md).

## How a request is served

```
request ─► conn limit ─► load shed ─► subnet rate limit ─► normalize to eTLD+1
        ─► not canonical? 301 to /{style}/{eTLD+1} ─► blocklist
        ─► memory cache ─► pinned seed ─► storage ─► hit:  SVG
                                    └► miss: fair queue ─► wait ≤ wait_ms
                                                          ├► done: SVG
                                                          └► generated icon, reason=queued
worker  ─► domain age (RDAP) ─► stream HTML until </head> ─► manifest (declared or guessed) ─► candidates
        ─► download (SSRF-safe, size-capped) ─► decode in sandboxed pool
        ─► sanitize SVG or trace bitmap (≤256 px) ─► storage + meta.json
```

The first request for a site nobody asked about yet usually gets a generated placeholder with reason `queued`; a moment later the real icon is cached. The [JavaScript client](../clients/javascript.md) waits that out for you.

## Next

- [Quick start](./quick-start.md)
- [HTTP API](../api/overview.md)
- [Self-hosting](../self-hosting/install.md)
