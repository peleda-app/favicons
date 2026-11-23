---
title: Render sandbox
description: The seccomp worker process, the isolated renderer container and the image.
---

# Render sandbox

Parsing attacker-controlled bytes is the most likely place for a memory-safety bug, even in Rust. So none of it runs inside the service that has network access, credentials and the cache: images, HTML `<head>` and manifests are all handled by a sandboxed worker. The parent re-checks everything the worker returns (format, sizes, URL safety, inline data limits), so a compromised worker can't inject internal URLs.

## Worker process

With `render.sandbox = "process"` (the default) the service starts `favicons render-worker` for every image, page or manifest: same binary, empty environment, cwd `/`, stderr discarded. The worker reads the request from stdin, then locks itself down before touching a byte:

| Layer | Setting |
| ----- | ------- |
| `RLIMIT_AS` | `render.worker_memory_bytes` (256 MiB) |
| `RLIMIT_CPU` | `decode_timeout_ms` rounded up |
| `RLIMIT_NOFILE` | 0: no new file descriptors |
| `RLIMIT_NPROC` | 0: no new processes or threads |
| `RLIMIT_CORE` | 0: no core dumps |
| `PR_SET_NO_NEW_PRIVS` | no privilege gain through exec |
| seccomp-bpf | about 20 allowed syscalls (memory, futex, write to stdout, exit); `open`, `socket`, `connect`, `execve`, `ptrace`, `fork` and the rest **kill the process** with `SIGSYS` |

The parent kills the worker on timeout, accepts at most 8 MiB of output and validates it before use. A seccomp kill is logged as such. Seccomp is Linux-only; on macOS the worker is isolated by the process boundary and rlimits, and the service warns at startup.

## Renderer container

With `render.sandbox = "socket"`, [docker-compose.yml](../../docker-compose.yml) runs the same image twice:

- `api`: network access, talks to the renderer over a unix socket on a shared volume.
- `renderer`: `favicons render-server` with `network_mode: none`, read-only root, `cap_drop: ALL`, `no-new-privileges`, uid 65534, `pids_limit: 32`, memory and CPU caps. The socket is `0600`. It still starts a fresh seccomp worker per image and replaces the limits sent by `api` with its own, so a compromised `api` can't loosen them.

## The image

`FROM scratch` with one statically linked (musl) binary: no shell, no libc, no package manager. Fetching uses root certificates compiled into the binary; the CA bundle in `/etc/ssl/certs` is only for the S3, GCS and Azure clients.

## Verify on your host

Each probe must be denied; on Linux it is killed with exit code 159 (128 + SIGSYS):

```bash
for p in fs write net exec prctl; do docker run --rm favicons/favicons sandbox-selftest $p; echo "$p: $?"; done
```

`sandbox-selftest render` must succeed with exit 0. CI runs the same checks.
