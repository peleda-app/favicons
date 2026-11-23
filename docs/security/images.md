---
title: Images and SVG
description: Decoder safety, bombs, the allowlist serializer and safe responses.
---

# Images and SVG

## Malicious images

- All decoders are memory-safe Rust (`image`, `png`, `zune-jpeg`, `image-webp`, `gif`, `resvg`/`usvg`, `visioncortex`). No libpng, libwebp, ImageMagick, Potrace or librsvg. The crate is built with `unsafe_code = "forbid"`.
- Format is chosen by magic bytes, never by `Content-Type`. Only PNG, ICO, JPEG, GIF (first frame), WebP, BMP and SVG are accepted.
- **Decompression bombs.** A 2 KiB PNG can claim 60000×60000 px. The header's declared size is checked against `render.max_source_dimension` (1024) before any pixel data is inflated, and `image::Limits` enforces the same bound plus a 32 MiB allocation cap inside every decoder. Tests cover PNG, PNG-in-ICO, WebP and GIF bombs; each must be rejected in under a second.
- Generic compressed containers (gzip, zlib, bzip2, xz, zstd, lz4, zip) are refused, which also closes `svgz`.
- Aspect ratios above 4:1 are rejected.
- **Every image is decoded in a fresh, [sandboxed process](./sandbox.md)**, at most `render.decode_concurrency` at once, each killed after `render.decode_timeout_ms`.
- **Nothing is passed through.** Every icon is rebuilt as a new SVG, so metadata chunks, trailing payloads, polyglots and embedded scripts don't survive.

## Malicious SVG

- ≤ 256 KiB, `<!ENTITY` declarations rejected, markup depth ≤ 48.
- `usvg` is built without text and system fonts. External references (`<image href>`, file paths, `http:`, `data:`) resolve to nothing.
- The output is written by an **allowlist serializer**. It can only emit `<svg>`, `<defs>`, `<path>` (`fill`, `fill-opacity`, `fill-rule`, `d`), `<linearGradient>`, `<radialGradient>` and `<stop>`, with numeric values and `#rrggbb` colours. Scripts, `<style>`, CSS, `<foreignObject>`, `<image>`, links, event handlers, animations, text, filters, masks and external references cannot be expressed.
- If the re-rendered result differs from the original by more than `render.svg_fidelity`, the icon is traced from pixels instead.
- **Second check outside the sandbox.** The parent re-parses every SVG the worker returns with a strict scanner: element and attribute allowlists per nesting level, value character classes, `url(#gN)` only, no comments, entities, text nodes or extra roots, and a size cap. Only then are gradient ids suffixed with a hash.
- Badges are composed around that checked SVG. Their only CSS is the server's own `<style>` with plate and mask colours and a `prefers-color-scheme` query, built from validated `#rrggbb` values.
- `render.svg = false` disables SVG input completely.

## Responses

- Always `Content-Type: image/svg+xml` with `X-Content-Type-Options: nosniff`.
- `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'none'; sandbox`: even opened as a document, nothing can execute or load. Inline styles stay allowed so badges can follow the system scheme.
- No cookies and `Referrer-Policy: no-referrer`.
- `Access-Control-Allow-Origin: *` and `Timing-Allow-Origin: *` without credentials. `Server-Timing` carries only the request duration.
