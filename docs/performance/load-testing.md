---
title: Load testing
description: The k6 suite that runs an attack and honest traffic at once.
---

# Load testing

[loadtest/](../../loadtest) holds a [k6](https://k6.io) suite. All four scenarios run at once against a release build:

| Scenario | Rate | What it proves |
| -------- | ---- | -------------- |
| `hot_cache` | 400 rps from random subnets | cache hits stay fast (p95 < 50 ms) |
| `junk_flood` | 300 rps of IPs, `localhost`, unknown TLDs, blocklisted hosts | junk is answered without any outbound request |
| `attacker` | 600 rps of unique domains from one `/24` | the subnet is rate-limited (> 50% `429`), no 5xx |
| `honest_during_attack` | 50 rps from other subnets | > 99% of honest requests still get the real icon |

```bash
./loadtest/run.sh -e DURATION=30s -e SCALE=1
```

It uses a local `k6` if installed, otherwise the `grafana/k6` image. The `loadtest` workflow runs it weekly, on pushes to `main` and on demand, and writes the summary to the job page.
