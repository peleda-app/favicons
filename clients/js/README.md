<div align="center">

<a href="https://favicons.page"><img src="https://raw.githubusercontent.com/peleda-app/favicons/main/docs/brand/banner.png" alt="favicons.page" width="100%"></a>

# favicons.page

**Any website's icon as a clean, safe SVG, with colours that fit every theme.**
<br>A zero-dependency client, React and Vue components, a Svelte action and a web component.

[![npm](https://img.shields.io/npm/v/favicons.page?color=ffb38a&label=npm)](https://www.npmjs.com/package/favicons.page)
[![minzipped](https://img.shields.io/bundlephobia/minzip/favicons.page?color=ffb38a&label=minzipped)](https://bundlephobia.com/package/favicons.page)
![dependencies](https://img.shields.io/badge/dependencies-0-ffb38a)
![types](https://img.shields.io/badge/types-included-ffb38a)
[![license](https://img.shields.io/badge/license-MIT-ffb38a)](https://github.com/peleda-app/favicons/blob/main/LICENSE)

[**Try it live**](https://favicons.page) · [HTTP API](https://github.com/peleda-app/favicons/blob/main/docs/API.md) · [Self-host](https://github.com/peleda-app/favicons/blob/main/docs/SELF-HOSTING.md) · [GitHub](https://github.com/peleda-app/favicons)

<img src="https://favicons.page/color/github.com/superellipse" width="44" height="44" alt="github.com"> <img src="https://favicons.page/color/discord.com/superellipse" width="44" height="44" alt="discord.com"> <img src="https://favicons.page/color/spotify.com/superellipse" width="44" height="44" alt="spotify.com"> <img src="https://favicons.page/color/figma.com/superellipse" width="44" height="44" alt="figma.com"> <img src="https://favicons.page/color/notion.so/superellipse" width="44" height="44" alt="notion.so"> <img src="https://favicons.page/color/stripe.com/superellipse" width="44" height="44" alt="stripe.com"> <img src="https://favicons.page/color/slack.com/superellipse" width="44" height="44" alt="slack.com"> <img src="https://favicons.page/color/youtube.com/superellipse" width="44" height="44" alt="youtube.com">
<br>
<img src="https://favicons.page/mono/github.com/rounded/light" width="44" height="44" alt="github.com"> <img src="https://favicons.page/mono/discord.com/rounded/light" width="44" height="44" alt="discord.com"> <img src="https://favicons.page/mono/spotify.com/rounded/light" width="44" height="44" alt="spotify.com"> <img src="https://favicons.page/mono/figma.com/rounded/light" width="44" height="44" alt="figma.com"> <img src="https://favicons.page/mono/notion.so/rounded/light" width="44" height="44" alt="notion.so"> <img src="https://favicons.page/mono/stripe.com/rounded/light" width="44" height="44" alt="stripe.com"> <img src="https://favicons.page/mono/slack.com/rounded/light" width="44" height="44" alt="slack.com"> <img src="https://favicons.page/mono/youtube.com/rounded/light" width="44" height="44" alt="youtube.com">

</div>

## Why

- **One URL per site.** No key, no quota, CORS open. Every response is a 128×128
  SVG that scales to any size, even when the site only has a 16 px `favicon.ico`.
- **Colours for both themes.** Each icon comes with a badge fill, a readable text
  colour and a mask tint for light and dark, checked against WCAG contrast.
- **Badges and masks.** Ready-made rounded and superellipse badges that switch
  with `prefers-color-scheme`, and single-colour masks that take your text colour.
- **Safe and private.** Images are rebuilt in a sandbox by an allowlist writer,
  so no scripts reach your page, and per-recipient subdomains never reach the site.
- **Yours to run.** One option points every helper at [your own instance](#your-own-instance).

<a href="https://favicons.page"><img src="https://cdn.jsdelivr.net/gh/peleda-app/favicons@main/docs/brand/link-previews.svg" alt="A mail client switching from light to dark, then to monochrome icons tinted in brand, orange and violet" width="100%"></a>

## Install

```bash
npm install favicons.page
```

| Import | What |
|--------|------|
| `favicons.page` | URLs, the SVG with its palette, catalogs |
| `favicons.page/react` | `<Favicon>`, `<FaviconsProvider>`, `useFavicon`, `useFaviconUrl` |
| `favicons.page/vue` | `<Favicon>`, `FaviconsPlugin`, `useFavicon`, `useFaviconUrl` |
| `favicons.page/svelte` | the `use:favicon` action |
| `favicons.page/element` | the `<favicon-icon>` web component, for any framework or none |

Each entry point brings only what it imports. React and Vue are optional peer
dependencies; nothing else is installed. Works in browsers, Node 18+, Deno, Bun
and edge runtimes.

## Quick start

```jsx
import { Favicon } from "favicons.page/react";

<Favicon domain="github.com" shape="superellipse" size={40} alt="GitHub" />
<Favicon domain="github.com" mono size="1em" />  {/* a mask in the text colour */}
```

```js
import { iconUrl, getIcon } from "favicons.page";

iconUrl("github.com"); // https://favicons.page/color/github.com
const { svg, palette } = await getIcon("discord.com");
palette.dark.plate;    // "#181b3f"
```

Without a bundler, load it from a CDN as an ES module:

```html
<script type="module">
  import { defineFaviconElement } from "https://cdn.jsdelivr.net/npm/favicons.page@0/element.js";
  defineFaviconElement();
</script>
<favicon-icon domain="github.com" shape="superellipse" size="40"></favicon-icon>
```

## URLs

Most apps only need a URL for an `<img>`. Building it makes no request:

```js
import { iconUrl } from "favicons.page";

iconUrl("github.com");
// https://favicons.page/color/github.com

iconUrl("https://www.github.com/features", { style: "mono" });
// https://favicons.page/mono/github.com

iconUrl("github.com", { shape: "superellipse" });
// a badge that follows the viewer's light or dark theme

iconUrl("github.com", { shape: "rounded", theme: "dark" });
// a badge pinned to the dark theme

iconUrl({ catalog: "selfhst", slug: "jellyfin" });
// https://favicons.page/color/selfhst/jellyfin
```

Domains can be given as a host, a full URL or a `URL`. International names
become punycode, a port, path and trailing dot are dropped. The server then
reduces any subdomain to the registrable domain.

| Option | Values | Default |
|--------|--------|---------|
| `style` | `"color"`, `"mono"` | `"color"` |
| `shape` | `"rounded"`, `"superellipse"` | no badge, the bare icon |
| `theme` | `"light"`, `"dark"`, `"auto"` | `"auto"`: follows `prefers-color-scheme`; needs a `shape` |

## The SVG and its colours

```js
import { getIcon } from "favicons.page";

const icon = await getIcon("discord.com");
icon.svg;                 // "<svg xmlns=…"
icon.palette.fill;        // "bleed": draw it edge to edge; "inset": give it padding
icon.palette.light.plate; // "#ebecfa", a badge colour for light themes
icon.palette.dark.text;   // "#6b77f3", the brand colour readable on dark
icon.source;              // "cache", "origin" or "generated"
icon.fallback;            // undefined, or { reason: "not-found" } and others
```

The first request for a site the server has never seen may come back as a
generated placeholder with reason `queued`. `getIcon` then waits and asks again
with `cache: "reload"`, up to `retries` times, and returns the real icon when it
is ready. Other fallbacks, such as `not-found` or `blocklisted`, are returned at
once, so you can show your own placeholder.

`headIcon(target, options)` returns the same fields without downloading the SVG.
Both accept `retries`, `retryDelay` and an `AbortSignal` as `signal`.

Statuses other than `200` throw a `FaviconsError` with `status`, `url` and the
`response`. `429` and `503` are retried first.

## Monochrome masks

`/mono` icons are black shapes meant for CSS masks, so they take the colour of
the text around them:

```js
import favicons from "favicons.page";

const css = favicons.mask("github.com");
// -webkit-mask: url("https://favicons.page/mono/github.com") center / contain no-repeat; mask: …

element.style.cssText = `width:16px;height:16px;background:currentColor;${css}`;
```

## Components

Every component takes the same props:

| Prop | Meaning | Default |
|------|---------|---------|
| `domain` | The site, as a host or a URL | |
| `catalog`, `slug` | An icon by name instead, e.g. `selfhst` and `jellyfin` | |
| `mono` | A single-colour icon. Without a `shape` it is a CSS mask in `currentColor`, so it takes the text colour around it | `false` |
| `shape` | `rounded` or `superellipse`: the icon on a badge | no badge |
| `theme` | `light`, `dark` or `auto` for the badge | `auto` |
| `size` | A number of pixels or any CSS length | `32` |
| `alt` | Text for screen readers; empty hides the icon from them | `""` |

Colour icons and badges render as `<img loading="lazy" referrerpolicy="no-referrer">`,
so browsers and CDNs cache them like any image.

### React

```jsx
import { Favicon, useFavicon } from "favicons.page/react";

<Favicon domain="github.com" shape="superellipse" size={40} alt="GitHub" />
<Favicon domain="github.com" mono size="1em" style={{ color: "rebeccapurple" }} />

function Sender({ domain }) {
  const { icon, loading } = useFavicon(domain);
  return <span style={{ color: icon?.palette?.light.text }}>{domain}</span>;
}
```

Other props, such as `className` or `onClick`, go to the element.

### Vue

```vue
<script setup>
import { Favicon, useFavicon } from "favicons.page/vue";
const props = defineProps(["domain"]);
const { icon } = useFavicon(() => props.domain);
</script>

<template>
  <Favicon :domain="domain" shape="rounded" theme="dark" :size="40" />
  <span :style="{ color: icon?.palette?.dark.text }">{{ domain }}</span>
</template>
```

`useFavicon` and `useFaviconUrl` take refs, getters or plain values and follow
their changes.

### Svelte

```svelte
<script>
  import { favicon } from "favicons.page/svelte";
  let { domain } = $props();
</script>

<img use:favicon={{ domain, shape: "superellipse", size: 40 }} alt={domain} />
<span use:favicon={{ domain, mono: true, size: "1em" }}></span>
```

On an `<img>` the action sets its `src`; on any other element it draws the icon
as a mask or a background.

### Web component

```js
import { defineFaviconElement } from "favicons.page/element";

defineFaviconElement(); // registers <favicon-icon>, or pass your own tag name
```

```html
<favicon-icon domain="github.com" shape="superellipse" size="40" alt="GitHub"></favicon-icon>
<favicon-icon domain="github.com" mono style="color: teal"></favicon-icon>
```

Attributes match the props above. The icon sits
in a shadow root as `part="icon"`, so `favicon-icon::part(icon)` styles it. In
server-side code `defineFaviconElement` does nothing and returns `undefined`.

## Server-side rendering

URLs are computed without a request and come out the same on the server and in
the browser, so `<Favicon>` renders straight into the server HTML and hydrates
without a mismatch. Hooks and composables fetch only in the browser and start in
their `loading` state on the server. When the first HTML needs the colours or
the SVG itself, call `getIcon` on the server; it waits out a `queued` icon there
too. Return `svg` and `palette` from loaders, not the whole result: its
`response` does not serialize.

### Next.js

`favicons.page/react` is marked `"use client"`, so server components can render
`<Favicon>` and fetch the palette themselves:

```jsx
import { getIcon } from "favicons.page";
import { Favicon } from "favicons.page/react";

export default async function Sender({ domain }) {
  const { palette } = await getIcon(domain);
  return (
    <span style={{ color: palette?.light.text }}>
      <Favicon domain={domain} size={20} /> {domain}
    </span>
  );
}
```

Pass a `fetch` of your own to cache the answers the way your app does:
`createFavicons({ fetch: (url, init) => fetch(url, { ...init, next: { revalidate: 86400 } }) })`.

### Nuxt

```vue
<script setup>
import { getIcon } from "favicons.page";
import { Favicon } from "favicons.page/vue";

const props = defineProps(["domain"]);
const { data: palette } = await useAsyncData(`favicon:${props.domain}`, () =>
  getIcon(props.domain).then((icon) => icon.palette),
);
</script>

<template>
  <Favicon :domain="domain" :size="20" />
  <b :style="{ color: palette?.light.text }">{{ domain }}</b>
</template>
```

### SvelteKit

Actions run only in the browser, so in server-rendered markup set the `src`
yourself:

```js
// +page.server.js
import { getIcon } from "favicons.page";

export async function load({ params }) {
  const { svg, palette } = await getIcon(params.domain);
  return { domain: params.domain, svg, palette };
}
```

```svelte
<script>
  import { iconUrl } from "favicons.page";
  let { data } = $props();
</script>

<img src={iconUrl(data.domain, { shape: "superellipse" })} width="40" height="40" alt="" />
<b style:color={data.palette?.light.text}>{data.domain}</b>
```

`{@html data.svg}` inlines the icon itself; size it with CSS, since every SVG
says `width="128"`. Inlining is safe: the server rebuilds each SVG from paths
and gradients only, and its ids are unique to the icon, so icons never clash in
one page.

### Web component

The server sends the bare tag and the icon appears once `defineFaviconElement()`
runs in the browser. Give the tag its size in CSS so nothing shifts:
`favicon-icon { width: 40px; height: 40px; }`.

## Your own badge

`shape` gives the two badges the server draws: `rounded` (22.5% corners) and
`superellipse`. For any other radius, put the bare icon on a plate of your own
and take its colours from the palette:

```js
const { palette } = await getIcon("discord.com");
palette.light.plate;     // badge fill for light themes
palette.light.text;      // text next to the icon, at least 4.5:1 on white
palette.dark.mono;       // tint for the mono mask on dark themes
palette.dark.monoPlate;  // badge fill under the mono mask
palette.fill;            // "bleed": fill the badge, "inset": pad the icon
palette.accent;          // the dominant brand colour
```

```jsx
import { Favicon, useFavicon } from "favicons.page/react";

function Badge({ domain, radius = 8, size = 40, dark = false }) {
  const { icon } = useFavicon(domain);
  const p = icon?.palette;
  const scheme = dark ? p?.dark : p?.light;
  return (
    <span style={{
      display: "inline-flex", width: size, height: size, borderRadius: radius,
      overflow: "hidden", background: scheme?.plate, boxSizing: "border-box",
      padding: p?.fill === "inset" ? size * 0.18 : 0,
    }}>
      <Favicon domain={domain} size="100%" />
    </span>
  );
}
```

Three things make it look right: `overflow: hidden` clips a `bleed` icon to
your corners, `inset` icons get about 18% padding, and the theme is yours to
pick, for example with `matchMedia("(prefers-color-scheme: dark)")`.

## Catalogs

Icons by name from the server's catalogs, such as Simple Icons and selfh.st:

```js
import favicons from "favicons.page";

const catalogs = await favicons.catalogs(); // [{ id, name, license, count }, …]
const { items } = await favicons.catalog("selfhst"); // [{ slug, name, category }, …]
favicons.url({ catalog: "selfhst", slug: items[0].slug }, { shape: "superellipse" });
```

## Your own instance

Everything talks to `https://favicons.page` unless you point it at a server of
your [own](https://github.com/peleda-app/favicons/blob/main/docs/SELF-HOSTING.md),
once per app:

| Entry point | How |
|-------------|-----|
| `favicons.page` | `const favicons = createFavicons({ baseUrl: "https://icons.example.com" })` |
| `favicons.page/react` | wrap the app in `<FaviconsProvider baseUrl="…">` |
| `favicons.page/vue` | `app.use(FaviconsPlugin, { baseUrl: "…" })` |
| `favicons.page/svelte` | pass `baseUrl` with the action's other options |
| `favicons.page/element` | `defineFaviconElement("favicon-icon", { baseUrl: "…" })` |

Components also take `baseUrl` (`base-url` on the web component) for a single
icon. `createFavicons` accepts a few more options:

| Option | Meaning | Default |
|--------|---------|---------|
| `baseUrl` | Where the API lives, with an optional path prefix | `https://favicons.page` |
| `fetch` | A `fetch` implementation for runtimes without a global one | `globalThis.fetch` |
| `retries` | Extra attempts while an icon is `queued` or the server is busy | `2` |
| `retryDelay` | Milliseconds between attempts, unless `Retry-After` says otherwise | `1500` |

## Reference

| Function | Returns |
|----------|---------|
| `iconUrl(target, { style, shape, theme })` | The canonical URL, without a request |
| `getIcon(target, options)` | `{ svg, palette, source, fallback, … }`, retrying `queued` |
| `headIcon(target, options)` | The same fields without the SVG |
| `createFavicons({ baseUrl, fetch, retries, retryDelay })` | A client with `url`, `get`, `head`, `mask`, `catalogs`, `catalog` |
| `normalizeDomain(input)` | The host as the server expects it |
| `maskCss(url, { size, position })` | `mask` and `-webkit-mask` declarations |

`target` is a domain, a URL or `{ catalog, slug }`. Response headers, fallback
reasons and caching rules are described in the
[HTTP API](https://github.com/peleda-app/favicons/blob/main/docs/API.md).

<a href="https://favicons.page"><img src="https://raw.githubusercontent.com/peleda-app/favicons/main/docs/brand/showcase.png" alt="Favicons of any site, icon packs, ready-made badges and monochrome masks" width="100%"></a>

## License

[MIT](https://github.com/peleda-app/favicons/blob/main/LICENSE). Logos stay
trademarks of their owners: show them to identify the site, not to imply
endorsement.
