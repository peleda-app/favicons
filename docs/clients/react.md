---
title: React
description: The Favicon component, hooks and Next.js.
---

# React

```jsx
import { Favicon, useFavicon } from 'favicons.page/react'

<Favicon domain="github.com" shape="superellipse" size={40} alt="GitHub" />
<Favicon domain="github.com" mono size="1em" style={{ color: 'rebeccapurple' }} />

function Sender({ domain }) {
  const { icon, loading } = useFavicon(domain)
  return <span style={{ color: icon?.palette?.light.text }}>{domain}</span>
}
```

Props are listed in [Component props](./components.md); others, such as `className` or `onClick`, go to the element. `useFaviconUrl` returns the URL alone. Wrap the app in `<FaviconsProvider baseUrl="…" private>` to set an instance or private mode once.

## Next.js

`favicons.page/react` is marked `"use client"`, so server components can render `<Favicon>` and fetch the palette themselves:

```jsx
import { getIcon } from 'favicons.page'
import { Favicon } from 'favicons.page/react'

export default async function Sender({ domain }) {
  const { palette } = await getIcon(domain)
  return (
    <span style={{ color: palette?.light.text }}>
      <Favicon domain={domain} size={20} /> {domain}
    </span>
  )
}
```

Pass your own `fetch` to cache answers the way your app does: `createFavicons({ fetch: (url, init) => fetch(url, { ...init, next: { revalidate: 86400 } }) })`.
