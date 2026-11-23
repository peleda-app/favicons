---
title: Palette headers
description: Shape, fill, accent and per-theme colours in the response headers.
---

# Palette headers

Colours to put around the icon, so it stays visible on any theme. Both styles carry the same set, computed from the colour icon:

```
x-icon-shape: circle
x-icon-fill: inset
x-icon-accent: #1ed760
x-icon-light-plate: #1c1c1e
x-icon-light-text: #13883d
x-icon-light-mono: #17a249
x-icon-light-mono-plate: #e7f8ed
x-icon-dark-plate: #183f26
x-icon-dark-text: #1ed760
x-icon-dark-mono: #1ed760
x-icon-dark-mono-plate: #1d442b
```

| Header | Meaning |
| ------ | ------- |
| `X-Icon-Shape` | Whether the colour icon brings its own background: `square`, `rounded`, `circle`, or `none` for a free-standing logo. Read from the outline, so a square with a transparent glyph cut out is still `square`. |
| `X-Icon-Fill` | `bleed`: the icon brings its own background to the edges (a square, or a rounded square with a corner radius up to 25% of its side); draw it at 100% and clip it to your corners. `inset`: a free-standing logo, a circle or a very round tile; draw it with padding. |
| `X-Icon-Accent` | The dominant brand colour; the darkest neutral for black-and-white logos. |
| `X-Icon-Light-*`, `X-Icon-Dark-*` | One scheme per theme. Light assumes a `#ffffff` background, dark `#1c1c1e`. |
| `…-Plate` | Badge fill behind the colour icon. Prefers a soft tint of the accent, then neutral, then the opposite brightness. For a free-standing logo it contrasts with most of the logo; for a tile with holes it contrasts with the tile, so a black square with a transparent glyph never gets a black plate. |
| `…-Text` | The accent, darkened or lightened to at least 4.5:1 against the theme background, for a name or link next to the icon. |
| `…-Mono` | Colour for the `mono` mask, at least 3:1 on its mono plate and therefore on the theme background. |
| `…-Mono-Plate` | Badge fill for the `mono` mask: a soft tint of the same hue close to the theme background. |

Inside a badge, give `inset` icons around 18% padding and draw `bleed` icons at full size.

The palette is computed once per fetch, in the sandbox, from a 64 px rendering of the final colour SVG, and stored in `meta.json`. An entry cached before palettes existed is served without them and refreshed in the background.
