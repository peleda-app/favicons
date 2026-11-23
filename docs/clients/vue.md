---
title: Vue
description: The Favicon component, composables, the plugin and Nuxt.
---

# Vue

```vue
<script setup>
import { Favicon, useFavicon } from 'favicons.page/vue'
const props = defineProps(['domain'])
const { icon } = useFavicon(() => props.domain)
</script>

<template>
  <Favicon :domain="domain" shape="rounded" theme="dark" :size="40" />
  <span :style="{ color: icon?.palette?.dark.text }">{{ domain }}</span>
</template>
```

`useFavicon` and `useFaviconUrl` take refs, getters or plain values and follow their changes. Props are listed in [Component props](./components.md). `app.use(FaviconsPlugin, { baseUrl: '…', private: true })` sets an instance or private mode once.

## Nuxt

```vue
<script setup>
import { getIcon } from 'favicons.page'
import { Favicon } from 'favicons.page/vue'

const props = defineProps(['domain'])
const { data: palette } = await useAsyncData(`favicon:${props.domain}`, () =>
  getIcon(props.domain).then((icon) => icon.palette),
)
</script>

<template>
  <Favicon :domain="domain" :size="20" />
  <b :style="{ color: palette?.light.text }">{{ domain }}</b>
</template>
```
