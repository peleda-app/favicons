# Performance

Measured with [wrk](https://github.com/wg/wrk) and
[`loadtest/throughput.lua`](../loadtest/throughput.lua), running on the same
machine as the service and talking to it over loopback. Every request asks
for a random popular icon in a random style from a random client subnet, so
the subnet rate limiter does its normal work; the icons are already cached.

```bash
wrk -t2 -c32 -d20s -s loadtest/throughput.lua http://127.0.0.1:8080
```

| Machine | Peak | p99 | favicons CPU | Memory | What gave out |
|---------|-----:|----:|-------------:|-------:|---------------|
| Apple M3 Pro laptop (12 cores) | 170 395 rps | 0.29 ms | 3.5 cores | 47 MB | wrk and favicons sharing the 6 performance cores |
| Intel Celeron N3160 thin client (4 × 1.6 GHz, 3.5 GB) | 10 843 rps | 6.2 ms | 2.0 cores | 63 MB | the CPU, half of it spent in the kernel's TCP stack |

No run had a single socket error or non-2xx response.

### Apple M3 Pro laptop

| Server threads | wrk | Throughput | p99 | p99.9 | favicons CPU | Memory |
|---------------:|-----|-----------:|----:|------:|-------------:|-------:|
| 12 (default) | 2 threads, 32 connections | 161 319 rps | 0.34 ms | 22.8 ms | 3.6 cores | 59 MB |
| 6 | 2 threads, 32 connections | 170 395 rps | 0.29 ms | 0.45 ms | 3.5 cores | 47 MB |
| 4 | 2 threads, 32 connections | 166 681 rps | 0.34 ms | 0.51 ms | 3.1 cores | 45 MB |

More wrk threads or connections only lowered throughput, and four or six
server threads did as well as twelve: wrk and favicons compete for the same 6
performance cores. Roughly 170 000 rps is the ceiling of one laptop serving and
loading itself; the service was not short of CPU.

### Intel Celeron N3160 thin client

favicons and wrk ran at low CPU priority.

| Server threads | wrk | Throughput | p99 | p99.9 | favicons CPU | Memory |
|---------------:|-----|-----------:|----:|------:|-------------:|-------:|
| 4 (default) | 1 thread, 8 connections | 7 775 rps | 2.9 ms | 5.0 ms | 1.6 of 4 cores | 63 MB |
| 4 (default) | 2 threads, 32 connections | 8 797–10 843 rps | 6.2–8.0 ms | 8.8–10.9 ms | 1.8–2.0 of 4 cores | 63 MB |
| 2 | 2 threads, 64 connections | 9 446 rps | 10.3 ms | 11.0 ms | 1.7 of 4 cores | 55 MB |

Here the box itself is the limit: during a run only 4% of its CPU was idle,
and almost half went to the kernel moving packets over loopback (32% system,
14% interrupts) rather than to favicons.

These are cache hits. Fetching a new site's icon is bounded by
`fetch.workers` and the network, not by these numbers.

## Load testing

[`loadtest/`](../loadtest) holds a [k6](https://k6.io) suite. All four scenarios
run at once against a release build:

| Scenario | Rate | What it proves |
|----------|------|----------------|
| `hot_cache` | 400 rps from random subnets | cache hits stay fast (p95 < 50 ms) |
| `junk_flood` | 300 rps of IPs, `localhost`, unknown TLDs, blocklisted hosts | junk is answered without any outbound request |
| `attacker` | 600 rps of unique domains from one `/24` | the subnet gets rate-limited (> 50% `429`), no 5xx |
| `honest_during_attack` | 50 rps from other subnets | > 99% of honest requests still get the real icon |

```bash
./loadtest/run.sh -e DURATION=30s -e SCALE=1
```

It uses a local `k6` if installed, otherwise the `grafana/k6` Docker image. The
`loadtest` workflow runs it weekly, on pushes to `main` and on demand, and
writes the summary to the job page.
