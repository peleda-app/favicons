---
title: Themes and palettes
description: The colours every icon brings for light and dark themes.
---

# Themes and palettes

Every icon response says whether the icon brings its own background, its brand colour, and for light and dark themes a badge fill that keeps it visible, a readable text colour and a colour for the monochrome mask. The full list is in [Palette headers](../api/palette.md).

```
x-icon-shape: circle
x-icon-fill: inset
x-icon-accent: #1ed760
x-icon-light-plate: #1c1c1e
x-icon-light-text: #13883d
x-icon-dark-plate: #183f26
x-icon-dark-text: #1ed760
```

## Follow the system or pin a theme

- **Automatic.** Leave the theme out, as in `/color/github.com/superellipse`, and the SVG carries both palettes and switches them with `prefers-color-scheme`. It flips the moment the operating system does.
- **Your own switch.** Pin `/light` or `/dark` and swap the URL. Every version has a stable URL and its own cache entry.
- **Any colour.** `/mono/{site}` is a mask: tint it with your accent or with `X-Icon-*-Mono`. `X-Icon-*-Text` and `X-Icon-*-Mono-Plate` colour the name and the chip around it.

```html
<img src="https://favicons.page/color/github.com/superellipse" width="40" height="40" alt="" />
<img src="https://favicons.page/color/github.com/superellipse/dark" width="40" height="40" alt="" />
```

## Bleed and inset

`X-Icon-Fill: bleed` marks icons with their own full background: draw them at 100% of your badge and clip them to your corners. `inset` icons are free-standing logos, circles or very round tiles: give them around 18% padding. See [Your own badge](./badges.md#your-own-badge).
