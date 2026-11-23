---
title: Catalog API
description: Icons by slug, catalog lists and supported index formats.
---

# Catalog API

## `GET /{style}/{catalog}/{slug}`

Catalogs configured with `[[catalogs]]` serve icons by name, e.g. `/color/selfhst/jellyfin`, `/mono/selfhst/jellyfin`, `/color/selfhst/jellyfin/superellipse/dark`. The response is the same as for a domain: an SVG, the palette headers, `ETag` and cache profiles, with `X-Icon-Provenance: catalog`, `X-Icon-License` (the icon's own license when the index has one, else the catalog's) and `X-Icon-Domain: @{catalog}.{slug}`. The `mono` style uses the catalog's black variant when it has one, otherwise it is derived.

| Status | When |
| ------ | ---- |
| `200` | The icon, or a generated one with reason `queued` on the first request. |
| `301` | Upper case, a `.svg` suffix or a query string; `Location` is the canonical path. |
| `404` | Unknown style, catalog or slug. Nothing is fetched for a slug the index does not list. |
| `503` | The catalog index has not loaded yet (`Retry-After: 5`). |

## Lists

`GET /catalogs.json` lists the configured catalogs (`id`, `name`, `license`, `attribution`, `count`). `GET /catalogs/{id}.json` adds `items`: every `slug` with its `name`, and `category` and `tags` when the index has them.

## Index formats

| `format` | Catalog | Notes |
| -------- | ------- | ----- |
| `selfhst` | [selfh.st icons](https://selfh.st/icons/), CC BY 4.0 | SVG, PNG when there is no SVG, black `-dark` variant for `mono` |
| `simple-icons` | [Simple Icons](https://simpleicons.org), CC0 | single-colour marks, tinted with each brand's colour for `color` |
| `dashboard-icons` | [Dashboard Icons](https://github.com/homarr-labs/dashboard-icons), Apache-2.0 | SVG or PNG; `mono` is derived |
| `devicon` | [Devicon](https://devicon.dev), MIT | the `original` variant, else `plain`, `wordmark` or `line` |
| `thesvg` | [theSVG](https://github.com/GLINCKER/thesvg), per icon | every collection; `licenses` keeps only the listed licenses |
| `iconify` | any [Iconify JSON](https://github.com/iconify/icon-sets) set, e.g. Fluent Emoji (MIT), Twemoji (CC BY 4.0), Noto Emoji (Apache-2.0) | SVGs come from the index; `mono_index_url` points at a second set with the same names for `mono` |

Logos stay trademarks of their owners whatever the file's license.
