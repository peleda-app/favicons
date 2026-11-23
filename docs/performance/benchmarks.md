---
title: Benchmarks
description: Cached icon throughput on a laptop and a thin client.
---

# Benchmarks

Measured with [wrk](https://github.com/wg/wrk) and [loadtest/throughput.lua](../../loadtest/throughput.lua) on the same machine as the service, over loopback. Every request asks for a random popular icon in a random style from a random client subnet, so the rate limiter does its normal work; the icons are already cached.

```bash
wrk -t2 -c32 -d20s -s loadtest/throughput.lua http://127.0.0.1:8080
```

| Machine | Peak | p99 | CPU | Memory | Limit |
| ------- | ---: | --: | --: | -----: | ----- |
| Apple M3 Pro laptop (12 cores) | 170 395 rps | 0.29 ms | 3.5 cores | 47 MB | wrk and favicons sharing 6 performance cores |
| Intel Celeron N3160 thin client (4 × 1.6 GHz) | 10 843 rps | 6.2 ms | 2.0 cores | 63 MB | the CPU, half in the kernel's TCP stack |

No run had a socket error or a non-2xx response. These are cache hits; fetching a new site is bounded by `fetch.workers` and the network.

## Apple M3 Pro

| Server threads | wrk | Throughput | p99 | p99.9 | CPU | Memory |
| -------------: | --- | ---------: | --: | ----: | --: | -----: |
| 12 (default) | 2 threads, 32 connections | 161 319 rps | 0.34 ms | 22.8 ms | 3.6 cores | 59 MB |
| 6 | 2 threads, 32 connections | 170 395 rps | 0.29 ms | 0.45 ms | 3.5 cores | 47 MB |
| 4 | 2 threads, 32 connections | 166 681 rps | 0.34 ms | 0.51 ms | 3.1 cores | 45 MB |

More wrk threads or connections only lowered throughput: wrk and favicons compete for the same cores. About 170 000 rps is the ceiling of one laptop loading itself; the service was not short of CPU.

## Intel Celeron N3160

favicons and wrk ran at low CPU priority.

| Server threads | wrk | Throughput | p99 | p99.9 | CPU | Memory |
| -------------: | --- | ---------: | --: | ----: | --: | -----: |
| 4 (default) | 1 thread, 8 connections | 7 775 rps | 2.9 ms | 5.0 ms | 1.6 of 4 | 63 MB |
| 4 (default) | 2 threads, 32 connections | 8 797–10 843 rps | 6.2–8.0 ms | 8.8–10.9 ms | 1.8–2.0 of 4 | 63 MB |
| 2 | 2 threads, 64 connections | 9 446 rps | 10.3 ms | 11.0 ms | 1.7 of 4 | 55 MB |

Here the box is the limit: only 4% of its CPU was idle, and almost half went to the kernel moving packets over loopback (32% system, 14% interrupts).
