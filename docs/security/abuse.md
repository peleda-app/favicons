---
title: Abuse resistance
description: Connection limits, rate limiting, the fair queue and negative caching.
---

# Abuse resistance

Defense in depth, cheapest first:

1. **Connections.** `server.max_connections` (excess sockets are closed at once), an HTTP/1 header read timeout against slowloris, a 32 KiB header buffer, HTTP/2 stream and header-list limits.
2. **Load shedding.** Beyond `server.max_concurrent_requests` in flight, `503` is returned instantly.
3. **Per-subnet token bucket.** Client IPs are reduced to `/24` (IPv4) or `/56` (IPv6), then hashed. Over the limit: `429`, no work done. The table is a bounded LRU, so rotating through IPv6 space can't exhaust memory.
4. **No long waits on the network.** Misses wait at most `fetch.wait_ms`, then get a generated icon with reason `queued`.
5. **Fair fetch queue.** Each subnet has its own bucket and workers serve buckets round-robin, so a client flooding a million domains gets the same share as one asking for one. Buckets are capped (`fetch.per_client_queue_capacity`). When the queue is full the **largest** bucket loses its newest entry; if the newcomer is the largest, it is rejected (`overloaded`).
6. **Deduplication.** Concurrent requests for one domain share a single fetch.
7. **Negative caching.** Failures and missing icons are cached (`*_ttl_secs`) and storage misses are remembered for `cache.absent_ttl_secs`.
8. **Generated icons** are cached in a bounded in-memory LRU.

The [k6 suite](../performance/load-testing.md) checks that honest clients keep getting real icons during an attack.
