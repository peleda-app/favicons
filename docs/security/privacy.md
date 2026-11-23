---
title: Privacy
description: Why the service cannot become a read receipt or tracking oracle.
---

# Privacy

The main client of this service is a mail client rendering sender avatars. The service must not become a read-receipt or tracking oracle.

| Leak | Mitigation |
| ---- | ---------- |
| Recipient's IP reaching the sender | Users never contact the sender's server; the service fetches on their behalf. |
| Per-recipient subdomains (`u-48213.spam.tld`) | Input is collapsed to the **registrable domain** with the Public Suffix List. Subdomains are never resolved or fetched. Private PSL sections (`*.github.io`, `*.workers.dev`, …) are collapsed too (`policy.collapse_private_suffixes`). |
| One fresh registrable domain per recipient | Domain-age gate and blocklists. The fetch is shared across all users and cached for `cache.ttl_secs`, so at most one request per domain per TTL reaches the sender. |
| Timing correlation | Queue, shared cache and `stale-while-revalidate` mean origin fetches aren't tied to a user request. A CDN decouples further. |
| BIMI and catalog | BIMI is a TXT lookup through the DoH resolver plus a download from the host the record names, which passes every foreign-host check. theSVG comes from GitHub and reveals only which brand slug is fetched. |
| Upstream providers | Off by default. When enabled, only the registrable domain is sent, and only for sites with no usable icon of their own. |
| Our own logs | Raw client IPs are never logged or stored. Rate limiting uses `blake3_keyed(random_per_process_key, subnet)`; the key never leaves memory and changes on restart. |
| Referer and cookies to origins | Outbound requests send no `Referer`, no cookies and a fixed User-Agent. |

For stricter needs, [private requests](../api/private.md) answer from the cache only and make no outbound connection at all.
