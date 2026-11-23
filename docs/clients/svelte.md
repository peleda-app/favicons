---
title: Svelte
description: The use:favicon action and SvelteKit.
---

# Svelte

```svelte
<script>
  import { favicon } from 'favicons.page/svelte'
  let { domain } = $props()
</script>

<img use:favicon={{ domain, shape: 'superellipse', size: 40 }} alt={domain} />
<span use:favicon={{ domain, mono: true, size: '1em' }}></span>
```

On an `<img>` the action sets its `src`; on any other element it draws the icon as a mask or a background. It takes the [component props](./components.md), including `baseUrl` and `private`.

## SvelteKit

Actions run only in the browser, so in server-rendered markup set the `src` yourself:

```js
// +page.server.js
import { getIcon } from 'favicons.page'

export async function load({ params }) {
  const { svg, palette } = await getIcon(params.domain)
  return { domain: params.domain, svg, palette }
}
```

```svelte
<script>
  import { iconUrl } from 'favicons.page'
  let { data } = $props()
</script>

<img src={iconUrl(data.domain, { shape: 'superellipse' })} width="40" height="40" alt="" />
<b style:color={data.palette?.light.text}>{data.domain}</b>
```

`{@html data.svg}` inlines the icon itself; size it with CSS.
