---
title: JavaScript client
description: URLs, the SVG with its palette, masks, catalogs and your own instance.
---

# JavaScript client

[`favicons.page`](https://www.npmjs.com/package/favicons.page) on npm has no dependencies and works in browsers, Node 18+, Deno, Bun and edge runtimes.

```bash
npm install favicons.page
```

| Import | What |
| ------ | ---- |
| `favicons.page` | URLs, the SVG with its palette, catalogs |
| `favicons.page/react` | [React](./react.md): `<Favicon>`, `<FaviconsProvider>`, `useFavicon`, `useFaviconUrl` |
| `favicons.page/vue` | [Vue](./vue.md): `<Favicon>`, `FaviconsPlugin`, `useFavicon`, `useFaviconUrl` |
| `favicons.page/svelte` | [Svelte](./svelte.md): the `use:favicon` action |
| `favicons.page/element` | [Web component](./web-component.md): `<favicon-icon>` |

Each entry point brings only what it imports. React and Vue are optional peer dependencies.

## URLs

Building a URL makes no request:

```js
import { iconUrl } from 'favicons.page'

iconUrl('github.com') // https://favicons.page/color/github.com
iconUrl('https://www.github.com/features', { style: 'mono' }) // https://favicons.page/mono/github.com
iconUrl('github.com', { shape: 'superellipse' }) // a badge that follows the viewer's theme
iconUrl('github.com', { shape: 'rounded', theme: 'dark' }) // a badge pinned to dark
iconUrl({ catalog: 'selfhst', slug: 'jellyfin' }) // https://favicons.page/color/selfhst/jellyfin
```

Domains can be a host, a full URL or a `URL`. International names become punycode; a port, path and trailing dot are dropped. The server then reduces any subdomain to the registrable domain.

| Option | Values | Default |
| ------ | ------ | ------- |
| `style` | `"color"`, `"mono"` | `"color"` |
| `shape` | `"rounded"`, `"superellipse"` | no badge |
| `theme` | `"light"`, `"dark"`, `"auto"` | `"auto"`, needs a `shape` |

## The SVG and its colours

```js
import { getIcon } from 'favicons.page'

const icon = await getIcon('discord.com')
icon.svg // "<svg xmlns=…"
icon.palette.fill // "bleed" or "inset"
icon.palette.light.plate // "#ebecfa"
icon.palette.dark.text // "#6b77f3"
icon.source // "cache", "origin" or "generated"
icon.fallback // undefined, or { reason: "not-found" } and others
```

The first request for a site the server has never seen may come back with reason `queued`. `getIcon` then waits and asks again with `cache: "reload"`, up to `retries` times. Other [fallbacks](../api/fallbacks.md) are returned at once, so you can show your own placeholder.

`headIcon(target, options)` returns the same fields without downloading the SVG. Both accept `retries`, `retryDelay` and an `AbortSignal` as `signal`. Statuses other than `200` throw a `FaviconsError` with `status`, `url` and `response`; `429` and `503` are retried first.

The palette fields mirror the [palette headers](../api/palette.md): `plate`, `text`, `mono` and `monoPlate` per theme, plus `fill` and `accent`.

## Masks

```js
import favicons from 'favicons.page'

const css = favicons.mask('github.com')
// -webkit-mask: url("https://favicons.page/mono/github.com") center / contain no-repeat; mask: …
element.style.cssText = `width:16px;height:16px;background:currentColor;${css}`
```

## Catalogs

```js
const catalogs = await favicons.catalogs() // [{ id, name, license, count }, …]
const { items } = await favicons.catalog('selfhst') // [{ slug, name, category }, …]
favicons.url({ catalog: 'selfhst', slug: items[0].slug }, { shape: 'superellipse' })
```

## Your own instance

Everything talks to `https://favicons.page` unless you point it at [your own server](../self-hosting/install.md), once per app:

| Entry point | How |
| ----------- | --- |
| `favicons.page` | `createFavicons({ baseUrl: "https://icons.example.com" })` |
| `favicons.page/react` | `<FaviconsProvider baseUrl="…">` |
| `favicons.page/vue` | `app.use(FaviconsPlugin, { baseUrl: "…" })` |
| `favicons.page/svelte` | `baseUrl` with the action's options |
| `favicons.page/element` | `defineFaviconElement("favicon-icon", { baseUrl: "…" })` |

Components also take `baseUrl` (`base-url` on the web component) for a single icon.

| Option | Meaning | Default |
| ------ | ------- | ------- |
| `baseUrl` | Where the API lives, with an optional path prefix | `https://favicons.page` |
| `fetch` | A `fetch` implementation | `globalThis.fetch` |
| `retries` | Extra attempts while an icon is `queued` or the server is busy | `2` |
| `retryDelay` | Milliseconds between attempts, unless `Retry-After` says otherwise | `1500` |
| `private` | Read the cache only | `false` |
| `privateBaseUrl` | The private host of your own instance | `https://private.favicons.page` with the default `baseUrl` |

## Private mode

`private: true` reads the cache only, so the site never learns that someone looked it up. A domain that is not cached yet gets the generated icon with reason `not-cached`. See [Private requests](../api/private.md).

```js
import { createFavicons } from 'favicons.page'

const favicons = createFavicons({ private: true })
favicons.url('github.com') // https://private.favicons.page/color/github.com
```

On your own instance set `privateBaseUrl` to a host from its `server.private_hosts`; without one, `get` and `head` send `X-Icon-Private: 1` instead and component URLs stay as they are.

## Server-side rendering

URLs come out the same on the server and in the browser, so components render into server HTML and hydrate without a mismatch. Hooks and composables fetch only in the browser and start in their `loading` state on the server. When the first HTML needs the colours or the SVG, call `getIcon` on the server; it waits out a `queued` icon there too. Return `svg` and `palette` from loaders, not the whole result: its `response` does not serialize.

Inlining `svg` is safe: the server rebuilds each SVG from paths and gradients only and its ids are unique to the icon. Size it with CSS, since every SVG says `width="128"`.

## Reference

| Function | Returns |
| -------- | ------- |
| `iconUrl(target, { style, shape, theme })` | The canonical URL, without a request |
| `getIcon(target, options)` | `{ svg, palette, source, fallback, … }`, retrying `queued` |
| `headIcon(target, options)` | The same fields without the SVG |
| `createFavicons(options)` | A client with `url`, `get`, `head`, `mask`, `catalogs`, `catalog` |
| `normalizeDomain(input)` | The host as the server expects it |
| `maskCss(url, { size, position })` | `mask` and `-webkit-mask` declarations |

`target` is a domain, a URL or `{ catalog, slug }`.
