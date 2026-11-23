---
title: Component props
description: The props shared by every framework component.
---

# Component props

Every component takes the same props:

| Prop | Meaning | Default |
| ---- | ------- | ------- |
| `domain` | The site, as a host or a URL | |
| `catalog`, `slug` | An icon by name instead, e.g. `selfhst` and `jellyfin` | |
| `mono` | A single-colour icon. Without a `shape` it is a CSS mask in `currentColor` | `false` |
| `shape` | `rounded` or `superellipse`: the icon on a badge | no badge |
| `theme` | `light`, `dark` or `auto` for the badge | `auto` |
| `size` | A number of pixels or any CSS length | `32` |
| `alt` | Text for screen readers; empty hides the icon from them | `""` |
| `private` | Read the cache only | `false` |
| `baseUrl` | Another instance for this icon | |

Colour icons and badges render as `<img loading="lazy" referrerpolicy="no-referrer">`, so browsers and CDNs cache them like any image.

- [React](./react.md)
- [Vue](./vue.md)
- [Svelte](./svelte.md)
- [Web component](./web-component.md)
