---
title: Machine-readable docs
description: OpenAPI, llms.txt, robots, security.txt, health and metrics.
---

# Machine-readable docs

All compiled into the binary. Names and absolute URLs come from `server.public_url`, else `https://{tls.domains[0]}`, else the request's `Host` header (validated, never cached).

| Path | What |
| ---- | ---- |
| `/` | JSON pointer: `name`, `version`, `description`, `docs`, `openapi`, `llms`, `example` and `source`. On favicons.page the website Worker answers `/` instead. |
| `/openapi.json` | OpenAPI 3.1, generated at start from the config: its origin, version and configured catalogs (as an enum). |
| `/llms.txt` | How to use the API, for language models and agents ([llmstxt.org](https://llmstxt.org)). |
| `/sitemap.xml` | The URLs of `/openapi.json` and `/llms.txt`. |
| `/robots.txt` | Lets search engines index the docs but not `/color/` and `/mono/`, and lets user-driven AI agents (`ChatGPT-User`, `Claude-User`, `Perplexity-User`, `MistralAI-User`) fetch everything. |
| `/.well-known/security.txt` | Where to report vulnerabilities ([RFC 9116](https://www.rfc-editor.org/rfc/rfc9116)), with an `Expires` six months ahead. |

The website at favicons.page (`/`, `/_site/*`, `/docs*`) is a separate Cloudflare Worker in front of the server; the server itself serves no HTML.

## Health and metrics

`GET /healthz` answers `200 ok` on the public and the admin listener.

`GET /metrics` is Prometheus text on the admin listener only (`server.admin_listen`). Never expose it publicly.

| Metric | Type |
| ------ | ---- |
| `favicons_requests_total` | counter |
| `favicons_served_cache_total`, `favicons_canonical_redirects_total` | counter |
| `favicons_rate_limited_total`, `favicons_shed_total` | counter |
| `favicons_fetch_{ok,not_found,failed,too_new}_total` | counter |
| `favicons_decode_errors_total` | counter |
| `favicons_fallback_total{reason}` | counter |
| `favicons_queue_length`, `favicons_fetch_running` | gauge |
| `favicons_blocklist_entries` | gauge |
