---
title: Storage and cache
description: Cache backends, TTLs, disk caps, prerendered icons and the storage layout.
---

# Storage and cache

## Backends

`cache.storage` picks where icons live:

| Value | Store |
| ----- | ----- |
| `none` | Memory only, lost on restart |
| `/var/cache/favicons` | A local directory (also `file://…`) |
| `s3://bucket/prefix` | S3 or any S3-compatible store (MinIO, R2, Ceph, …) |
| `gs://bucket/prefix` | Google Cloud Storage |
| `az://container/prefix` | Azure Blob Storage |

`[cache.options]` is passed to `object_store` for regions, endpoints and credentials. TTLs are separate for real icons (`ttl_secs`, 7 days), missing icons, failures, too-new and blocked domains. Storage misses are remembered in memory for `absent_ttl_secs`.

On a local disk, cap the cache with `cache.max_disk_bytes` or `cache.max_disk_entries`: the least recently used domains are deleted until usage is back under 90% of the cap. The cap costs no memory per icon; a background pass walks the directory when usage crosses it, at most once a minute, otherwise once an hour. On object storage use lifecycle rules.

## Prerendered icons

`cache.seed` points at a read-only directory consulted when the working cache has no entry: `.pack` files made by `favicons pack DIR OUT.pack`, then a `v4/` tree. Entries there are pinned: never refreshed, re-rendered or evicted, and they win over a copy of the same domain in the working cache.

The `favicons.page` image ships the icons of the 100 000 most popular sites, combined from Majestic Million, the Cisco Umbrella list and the Chrome UX Report ([catalogs/sites.py](../../catalogs/sites.py), sources and licenses in `/catalogs/SOURCES.md` inside the image). A few `.pack` files, one sorted index plus the data each, hold them in a few hundred megabytes and a handful of inodes. A weekly workflow renders the list again in 20 parallel jobs, without the domain-age check. Only sites with an icon are kept; the rest are left to the server.

## Layout

```
{prefix}/v4/{domain}/meta.json
{prefix}/v4/{domain}/{color|mono}.svg
```

A new layout version starts with an empty cache. On a local directory the server removes older layouts (`v3`, …) in the background on start; object stores keep them until you delete them.

```json
{
  "domain": "github.com",
  "fetched_at": 1795132800,
  "status": "ok",
  "color": "original",
  "mono": "original",
  "mono_source": "mask-icon",
  "etags": { "color.svg": "…", "mono.svg": "…" }
}
```

`status` is one of `ok`, `not-found`, `fetch-failed`, `too-new`, `blocked`. Each slot holds the vector origin (`original` / `traced`) or `null`. Prerendered entries carry `"pinned": true`. The service always serves icons itself; storage is never exposed to clients.
