---
title: Badges and shapes
description: Server-drawn rounded and superellipse badges, and building your own.
---

# Badges and shapes

`/{style}/{domain}/{shape}/{theme}` returns the icon already drawn on its plate.

| Segment | Values |
| ------- | ------ |
| `shape` | `rounded` (22.5% corner radius) or `superellipse` (n = 5, like iOS app icons) |
| `theme` | `light`, `dark`, or left out to follow the viewer's scheme |

```html
<img src="https://favicons.page/color/github.com/superellipse/light" width="40" height="40" alt="" />
<img src="https://favicons.page/mono/selfhst/jellyfin/rounded" width="40" height="40" alt="" />
```

The plate takes the theme's `…-Plate` colour, `inset` icons get 18% padding and `bleed` icons fill the shape and are clipped to it. With `mono` the mask is tinted with `…-Mono` on the `…-Mono-Plate` colour. Catalog icons work the same way.

Without a theme the SVG carries both themes' colours and switches them with a `prefers-color-scheme` media query. Details are in [Badge routes](../api/overview.md#badges).

## Your own badge

For any other radius, put the bare icon on a plate of your own and take its colours from the [palette](../api/palette.md):

```jsx
import { Favicon, useFavicon } from 'favicons.page/react'

function Badge({ domain, radius = 8, size = 40, dark = false }) {
  const { icon } = useFavicon(domain)
  const p = icon?.palette
  const scheme = dark ? p?.dark : p?.light
  return (
    <span
      style={{
        display: 'inline-flex',
        width: size,
        height: size,
        borderRadius: radius,
        overflow: 'hidden',
        background: scheme?.plate,
        boxSizing: 'border-box',
        padding: p?.fill === 'inset' ? size * 0.18 : 0,
      }}
    >
      <Favicon domain={domain} size="100%" />
    </span>
  )
}
```

Three things make it look right: `overflow: hidden` clips a `bleed` icon to your corners, `inset` icons get about 18% padding, and the theme is yours to pick, for example with `matchMedia('(prefers-color-scheme: dark)')`.
