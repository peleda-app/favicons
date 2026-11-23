---
title: Quick start
description: Show an icon, a badge and a mask in a minute.
---

# Quick start

## An icon

```html
<img src="https://favicons.page/color/discord.com" width="32" height="32" alt="" />
```

Every response is a 128×128 SVG fitted edge to edge, so set the size you need.

## A badge

The server can draw the icon on a rounded square or an iOS-style superellipse. Without a theme the badge follows the viewer's light or dark scheme; add `/light` or `/dark` to pin one.

```html
<img src="https://favicons.page/color/discord.com/superellipse" width="40" height="40" alt="" />
<img src="https://favicons.page/color/discord.com/superellipse/dark" width="40" height="40" alt="" />
```

## A mask

For toolbars and tab bars, take the monochrome shape and tint it with CSS:

```css
.icon {
  background: currentColor;
  mask: url(https://favicons.page/mono/discord.com) center / contain no-repeat;
}
```

## The colours

Every icon response carries its palette in headers:

```
x-icon-shape: square            x-icon-light-plate: #ebecfa
x-icon-fill: bleed              x-icon-dark-plate:  #181b3f
x-icon-accent: #5a67f2          x-icon-dark-text:   #6b77f3
```

## From JavaScript

```bash
npm install favicons.page
```

```js
import { iconUrl, getIcon } from 'favicons.page'

iconUrl('github.com', { shape: 'superellipse' })
// https://favicons.page/color/github.com/superellipse

const icon = await getIcon('discord.com')
icon.palette.dark.plate // "#181b3f"
```

See the [JavaScript client](../clients/javascript.md) and the components for [React](../clients/react.md), [Vue](../clients/vue.md), [Svelte](../clients/svelte.md) and the [web component](../clients/web-component.md).

## Agents and automations

[`/llms.txt`](https://favicons.page/llms.txt) explains the API to language models and [`/openapi.json`](https://favicons.page/openapi.json) describes it for tools. See [Machine-readable docs](../api/machine-readable.md).
