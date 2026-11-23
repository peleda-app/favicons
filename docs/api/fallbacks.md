---
title: Fallbacks
description: Why a generated icon was returned and what it looks like.
---

# Fallbacks

When there is no real icon the response is still a `200` SVG, with `X-Icon-Fallback: true` and a reason:

| Reason | Cache profile | Meaning |
| ------ | ------------- | ------- |
| `queued` | `fallback_transient` | Not cached yet. A fetch is queued; retry later. |
| `overloaded` | `fallback_transient` | The fetch queue is full and this client already has more queued work than others. |
| `fetch-failed` | `fallback_transient` | The site was unreachable (DNS, TLS, timeouts, private IPs). Retried after `cache.fetch_failed_ttl_secs`. |
| `not-found` | `fallback_stable` | The site answered but exposes no usable icon. Retried after `cache.not_found_ttl_secs`. |
| `domain-too-new` | `fallback_stable` | RDAP registration date is younger than `policy.min_domain_age_days`. |
| `blocklisted` | `fallback_stable` | The host or a parent is on a configured blocklist. `X-Icon-Blocked: blocklist`. |
| `dns-blocked` | `fallback_stable` | The filtering DNS resolver sinkholed the domain. `X-Icon-Blocked: dns-filter`. |
| `no-monochrome` | `fallback_stable` | `mono` was requested and neither a mask icon nor a usable derived shape exists. |
| `invalid-domain` | `fallback_stable` | Not a public, registrable DNS name (IP literals, `localhost`, unknown TLDs, `.onion`, …). |
| `not-cached` | `no-store` | A [private request](./private.md) for a domain that is not cached. |

## Generated icons

Blocked domains get a fixed **warning icon**: a red square and a white exclamation mark, the same for every domain. Clients can match on `X-Icon-Blocked` to show their own warning.

All other generated icons are deterministic: `blake3(identicon.salt || 0x00 || canonical_domain)` seeds a symmetric 5×5 white pattern on a gradient with three soft radial glows. The seed picks a harmonious hue scheme, the gradient direction and every glow; lightness is capped on yellow and green hues so the pattern stays readable. The same domain gets the same image on every instance that shares the salt.

## Loops

Every request the fetcher makes carries `X-Favicons-Fetch: 1`. A request with that header gets an icon only if one is already cached; anything else is a `404` with no fallback image and nothing queued. A site whose icon points back at this server or another favicons instance can neither recurse nor end up with a generated icon as its own.
