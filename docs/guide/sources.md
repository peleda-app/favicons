---
title: Where icons come from
description: Discovery order, curated sources, tracing and the SVG the service writes.
---

# Where icons come from

## Sources

Sources are tried in this order, and the first usable one wins:

1. **BIMI** (`bimi.enabled`, on by default). The domain's `default._bimi.<domain>` TXT record names the logo its owner publishes for mail clients, often backed by a Verified Mark Certificate. The certificate is not validated; the record itself is the owner's statement.
2. **theSVG catalog** (`catalog.thesvg`, off by default). Curated brand SVGs. The index is synced daily, and an entry matches only when its `url` is the site's root. Only the configured licenses are used (`CC0-1.0` by default) and the license is returned in `X-Icon-License`. The catalog's `mono` variant becomes the monochrome icon.
3. **The site itself.** See [Discovery](#discovery).
4. **Upstream providers**, if configured. See [Upstream providers](../self-hosting/configuration.md#upstream-providers).

When BIMI or the catalog delivers an icon, the site's own pages are not fetched at all. `X-Icon-Provenance` says which source won. Every source, curated or not, is untrusted input: it gets the same host checks, sandboxed parsing and allowlist serializer.

Brand logos are trademarks regardless of the license of the SVG file. Showing a site's own mark next to its address is the usual use, but check whether your use case is covered.

## Discovery

The worker reads `<link rel=icon|apple-touch-icon>`, web app manifests, `mask-icon`, inline `data:` icons, `/apple-touch-icon.png` and `/favicon.ico`. Only the HTML `<head>` is downloaded. When the page declares no manifest and offers nothing ≥128 px (or HTML is blocked by bot protection), `/manifest.json`, `/site.webmanifest` and `/manifest.webmanifest` are tried. The largest usable candidate wins.

Icons on CDNs or other domains are used only after that host passes the same blocklist, domain-age, DNS-filter and SSRF checks as the requested domain, and so does every redirect hop. By default only `https` on port 443 is used, and links or redirects to raw IP addresses are ignored (`fetch.allowed_ports`, `fetch.allow_ip_hosts`). See [Assets on other domains](../security/model.md#assets-and-redirects-on-other-domains).

## Vector output

Every response body is an SVG produced by the service, never the site's own file. `X-Icon-Vector` says how it was made:

| Value       | Meaning |
| ----------- | ------- |
| `original`  | The site published an SVG (`rel=icon type=image/svg+xml`, `mask-icon`, manifest). It was parsed in the sandbox and **re-serialized by an allowlist writer**: only `<path>` with solid fills, linear and radial gradients survive. Strokes become outlines and transforms are baked into coordinates. The result is rendered and compared with the original; if too much was lost (patterns, masks, clip paths), the icon is traced instead. |
| `traced`    | Traced with [VTracer](https://github.com/visioncortex/vtracer)'s engine (`visioncortex`) from the largest bitmap the site offers (capped at `render.trace_max_px`, 256 px). The bitmap is resampled to 512 px and reduced to at most 12 dominant colours, with anti-aliasing blends dropped, then traced into stacked spline paths. Shapes of one colour become one path where z-order allows. Smooth gradients become a few flat bands. |
| `generated` | Identicon or blocked-domain warning, drawn as vector. See [Fallbacks](../api/fallbacks.md). |

Every SVG is `128×128` (`viewBox="0 0 128 128"`). Its content is **fitted to the box**: scaled until it touches the edges on its longer side, then centred. A full-size plain white backdrop counts as padding and is removed first, so a logo on a white app tile ends up as large as a logo without padding. Coloured tiles are part of the design and kept. Output is capped at `render.max_output_svg_bytes` (48 KiB).

## Safe to inline

Gradient ids look like `g0-941ba8` and the badge's classes and ids like `.p-bf9106`, with a short hash of the icon, so any number of SVGs can be pasted into one page without one restyling or borrowing gradients from another.
