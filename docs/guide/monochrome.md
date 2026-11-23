---
title: Monochrome masks
description: Single-colour icons for CSS masks that take any tint.
---

# Monochrome masks

`/mono/{domain}` returns **black (`#000`) paths on transparency**, meant for CSS masks, so you pick the colour.

```css
.site-icon {
  width: 16px;
  height: 16px;
  background-color: currentColor;
  -webkit-mask: url('https://favicons.page/mono/github.com') center / contain no-repeat;
  mask: url('https://favicons.page/mono/github.com') center / contain no-repeat;
}
```

When a derived mask has parts in clearly different colours, such as a logo's letters, a ribbon's folds or a mascot's eyes, they come back as two or three stacked layers with `fill-opacity`, so the mask shows them as lighter shades of the one tint. A site's own `mask-icon` is kept as a single shape.

## Where the shape comes from

`X-Icon-Mono-Source`, best first:

| Value       | Meaning |
| ----------- | ------- |
| `mask-icon` | The site's own monochrome asset: `<link rel="mask-icon">` (Safari pinned tab SVG) or a manifest icon with `purpose: monochrome`. Only its alpha channel is used, so the declared `color` doesn't matter. |
| `derived`   | Converted from the colour icon. A logo on an opaque tile becomes the logo without the tile. A logo on a flat plate (white circle, coloured disc) keeps the plate with the details knocked out. A logo on transparency becomes its silhouette. |
| `generated` | A monochrome version of the generated fallback (identicon, or the `!` of the blocked-domain warning). Comes with `X-Icon-Fallback`. |

When the site has a colour icon but no shape could be derived from it (too little contrast), the response is a generated mono icon with reason `no-monochrome`.

The `…-Mono` and `…-Mono-Plate` [palette headers](../api/palette.md) give a tint and a badge fill for each theme.
