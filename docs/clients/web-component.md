---
title: Web component
description: The favicon-icon element for any framework or none.
---

# Web component

```js
import { defineFaviconElement } from 'favicons.page/element'

defineFaviconElement() // registers <favicon-icon>, or pass your own tag name
```

```html
<favicon-icon domain="github.com" shape="superellipse" size="40" alt="GitHub"></favicon-icon>
<favicon-icon domain="github.com" mono style="color: teal"></favicon-icon>
```

Without a bundler, load it from a CDN as an ES module:

```html
<script type="module">
  import { defineFaviconElement } from 'https://cdn.jsdelivr.net/npm/favicons.page@0/element.js'
  defineFaviconElement()
</script>
```

Attributes match the [component props](./components.md), with `base-url` for `baseUrl`. `defineFaviconElement('favicon-icon', { baseUrl: '…' })` sets an instance for every element. The icon sits in a shadow root as `part="icon"`, so `favicon-icon::part(icon)` styles it.

## Server-side rendering

In server-side code `defineFaviconElement` does nothing and returns `undefined`. The server sends the bare tag and the icon appears once the element is defined in the browser. Give the tag its size in CSS so nothing shifts: `favicon-icon { width: 40px; height: 40px; }`.
