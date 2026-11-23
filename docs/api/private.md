---
title: Private requests
description: Cache-only lookups that never contact the site.
---

# Private requests

A private request reads the cache and nothing else. The service makes no outbound connection for it: no fetch, no queue entry, no background refresh of a stale icon, no DNS, RDAP or BIMI lookup, no upstream provider and no catalog download. Nobody outside the instance learns that the domain was looked up.

- A cached icon is served as usual, stale or not.
- Anything else gets the generated icon with `X-Icon-Fallback-Reason: not-cached` and `Cache-Control: no-store` plus `CDN-Cache-Control: no-store`, so a CDN never hands that placeholder to ordinary requests.
- Badges, palettes and masks are computed locally from the cached icon.

Two ways to ask for it:

- **A private host.** Requests whose `Host` is listed in `server.private_hosts` are private. On favicons.page that is `https://private.favicons.page/color/github.com`. It works in a plain `<img src>` and keeps its own CDN cache.
- **A header.** `X-Icon-Private: 1` on any request, for `fetch()` and servers. Cross-origin browser requests with it go through a CORS preflight, which the service allows.

The popular sites and catalogs prerendered into the favicons.page image are always cached, so they are always available privately. The JavaScript client has a [private mode](../clients/javascript.md#private-mode).
