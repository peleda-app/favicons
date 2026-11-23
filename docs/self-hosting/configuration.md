---
title: Configuration
description: The config file, environment variables, catalogs and upstream providers.
---

# Configuration

[config.example.toml](../../config.example.toml) lists every option with its default. Every key can also be set as an environment variable `FAVICONS_<SECTION>__<KEY>`, for example `FAVICONS_FETCH__WORKERS=4`.

| Variable | What |
| -------- | ---- |
| `FAVICONS_CONFIG` | Path to the TOML file |
| `FAVICONS_LOG` | Log filter, e.g. `info,favicons=debug` |
| `FAVICONS_LOG_FORMAT=json` | JSON logs |
| `FAVICONS_CACHE__STORAGE` | `none`, a directory, or `s3://`, `gs://`, `az://` ([storage](./storage.md)) |
| `FAVICONS_TLS__ENABLED`, `FAVICONS_TLS__DOMAINS` | Built-in HTTPS ([TLS](./tls.md)) |
| `FAVICONS_POLICY__BLOCKLISTS` | Hosts, adblock or plain-domain lists to refuse |
| `FAVICONS_SERVER__PRIVATE_HOSTS` | Hosts that answer from the cache only ([private requests](../api/private.md)) |

The server is the API only. `/` answers with a small JSON pointer; `server.public_url` names the origin used there and in `llms.txt`, `robots.txt`, `sitemap.xml` and `openapi.json`. The old `server.landing` key is still accepted and ignored.

## Policy

- `policy.blocklists`: local files or URLs in hosts, adblock (`||domain^`) or plain formats, reloaded periodically. Suggested lists: abuse.ch URLhaus / ThreatFox, Phishing Army, OISD; none are bundled.
- `policy.min_domain_age_days`: domains younger than this (30 by default) are never fetched.
- `policy.allowlist`: overrides blocklists and domain age.

See [Malicious domains](../security/model.md#malicious-or-throw-away-domains).

## Catalogs

Each `[[catalogs]]` entry adds icons by slug, served at `/{style}/{id}/{slug}`:

```toml
[[catalogs]]
id = "selfhst"
name = "selfh.st icons"
format = "selfhst"
index_url = "https://cdn.jsdelivr.net/gh/selfhst/icons@main/index.json"
base_url = "https://cdn.jsdelivr.net/gh/selfhst/icons@main/"
license = "CC-BY-4.0"
attribution = "https://selfh.st/icons/"
```

Icons go through the same checks, sanitizer, palettes and caches as domains. Iconify catalogs are prerendered into the cache in the background when a storage is configured; `prerender = false` renders on first request instead. Formats are listed in [Catalog API](../api/catalogs.md#index-formats).

## Upstream providers

Optional and off by default. When the site itself offers no usable icon, or cannot be reached, the URL templates in `upstream.providers` are tried in order, with `{domain}` replaced by the registrable domain:

```toml
[upstream]
providers = [
  "https://favicon.vemetric.com/{domain}?size=256&format=webp",
  "https://icons.duckduckgo.com/ip3/{domain}.ico",
  "https://www.google.com/s2/favicons?domain={domain}&sz=64",
]
```

::: warning
These URLs are only examples of the template format. They are third-party services with their own terms of use, which may not allow this kind of access. Check their terms before enabling one, or point this at a service you run yourself.
:::

Upstream responses get the same treatment as any foreign host: https only, SSRF rules, blocklists, domain age, DNS filter, checked redirects, size caps. A non-2xx answer moves on to the next provider. The image is vectorized in the sandbox and the response carries `X-Icon-Upstream`.

Providers are never asked about a domain that does not resolve. Once a day each provider is asked about a random domain that does not exist; an answer identical to that placeholder is skipped as if it were a `404`.
