---
title: Install
description: Docker Compose, a single container, release binaries or cargo.
---

# Install

## Docker Compose

```bash
curl -O https://raw.githubusercontent.com/peleda-app/favicons/main/docker-compose.yml
docker compose up -d
```

This starts `api` and an isolated `renderer` with no network, a read-only root, no capabilities and a seccomp-confined worker process per image. See [Render sandbox](../security/sandbox.md).

## A single container

Images are still decoded in seccomp-confined child processes:

```bash
docker run -d --name favicons -p 8080:8080 \
  --read-only --cap-drop ALL --security-opt no-new-privileges \
  -v favicons-cache:/var/cache/favicons \
  -e FAVICONS_CACHE__STORAGE=/var/cache/favicons \
  favicons/favicons
```

```bash
curl -sI http://localhost:8080/color/github.com
```

```
HTTP/1.1 200 OK
content-type: image/svg+xml
etag: "b154cb1768af4678af57"
cache-control: public, max-age=604800, stale-while-revalidate=86400
cdn-cache-control: public, max-age=2592000, stale-while-revalidate=604800, stale-if-error=604800
x-icon-source: cache
x-icon-domain: github.com
x-icon-style: color
x-icon-vector: original
```

## Images

| Image | What |
| ----- | ---- |
| `favicons/favicons`, `ghcr.io/peleda-app/favicons` | The API alone: a `scratch` image with one static binary for amd64 and arm64, running as uid 65534, about 9 MB to download. |
| `ghcr.io/peleda-app/favicons.page` | The same API as the base of what runs favicons.page, with every catalog and the [top 100 000 sites](./storage.md#prerendered-icons) prerendered on top. |

| Tag | Follows |
| --- | ------- |
| `latest`, `main` | The main branch |
| `1.2`, `1.2.3` | Release tags |
| `sha-<commit>` | One exact build |

Every image is signed; see [Verifying builds](../security/hardening.md#verifying-builds).

## Binaries and cargo

Each [release](https://github.com/peleda-app/favicons/releases) carries static binaries as `favicons-<version>-<arch>-linux-musl` (x86_64 and aarch64) with a `SHA256SUMS` file. `cargo install favicons` builds the API from crates.io. From a checkout:

```bash
cargo run --release -- --config config.example.toml
```

## Next

- [Configuration](./configuration.md)
- [Storage and cache](./storage.md)
- [HTTPS](./tls.md)
- [Small servers](./small-server.md)
