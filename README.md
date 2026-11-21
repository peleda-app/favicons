<div align="center">

![favicons.page](https://stats.favicons.page/banner.png)

**Any website's icon as a clean, safe SVG, with colours that fit every theme.**

[![CI](https://github.com/peleda-app/favicons/actions/workflows/ci.yml/badge.svg)](https://github.com/peleda-app/favicons/actions/workflows/ci.yml)
[![Docker](https://img.shields.io/badge/ghcr.io-peleda--app%2Ffavicons-2496ED?logo=docker&logoColor=white)](https://github.com/peleda-app/favicons/pkgs/container/favicons)
[![Docker Hub](https://img.shields.io/badge/docker.io-favicons%2Ffavicons-2496ED?logo=docker&logoColor=white)](https://hub.docker.com/r/favicons/favicons)
[![License: MIT](https://img.shields.io/badge/license-MIT-ffb38a)](LICENSE)

[**Try it on favicons.page**](https://favicons.page) · [API](docs/API.md) · [Self-hosting](docs/SELF-HOSTING.md) · [Security](docs/SECURITY.md)

</div>

## One URL per site

```html
<img src="https://favicons.page/color/discord.com" width="32" height="32" alt="">
```

No key, no query parameters, CORS open. Every response is an edge-to-edge SVG,
and its headers carry the colours to put around it:

```
x-icon-shape: square            x-icon-light-plate: #ebecfa
x-icon-fill: bleed              x-icon-dark-plate:  #181b3f
x-icon-accent: #5a67f2          x-icon-dark-text:   #6b77f3
```

Or let the server draw the badge for you, on a rounded square or an iOS-style
superellipse in the right colour for each theme:

```html
<img src="https://favicons.page/color/discord.com/superellipse/light" width="40" height="40" alt="">
```

Need a single-colour icon for a toolbar or a tab bar? Ask for the mask and tint it with CSS:

```css
.icon {
  background: currentColor;
  mask: url(https://favicons.page/mono/discord.com) center / contain no-repeat;
}
```

## Why favicons

- **Vector, always.** Sites that ship SVG keep their exact shapes. Bitmaps are
  traced into smooth vectors, so even a 32 px `favicon.ico` scales to any size.
- **Looks right on light and dark.** Badge, text and mask colours for both
  themes, checked against WCAG contrast. No more black logo on a black row.
- **The real logo first.** The owner's BIMI mail logo, then the site's own icons
  and manifests, then optional fallback providers. A generated identicon only
  when nothing exists, and the headers say why.
- **Built to be attacked.** Every image is decoded in a throw-away seccomp
  sandbox with no files, network or exec, then rebuilt by an allowlist SVG
  writer. No SSRF, no decompression bombs, no scripts in your icons.
- **Private by design.** Requests collapse to the registrable domain, so
  per-recipient subdomains can't become read receipts. Lookups go over
  encrypted, malware-filtering DNS. Client IPs are never logged.
- **Fast and small.** About 170 000 cached icons per second on a laptop,
  10 000 on a 4-core Celeron thin client, in about 50 MB of RAM.
  [Benchmarks](docs/PERFORMANCE.md).

## Self-hosted apps by name

Optional catalogs serve icons by slug, with the same palettes, masks and plates.
favicons.page carries about 17 000 of them: [Simple Icons](https://simpleicons.org),
[selfh.st](https://selfh.st/icons/), [Dashboard Icons](https://github.com/homarr-labs/dashboard-icons),
[Devicon](https://devicon.dev) and [theSVG](https://github.com/GLINCKER/thesvg), each
response naming its license:

```html
<img src="https://favicons.page/color/simple-icons/github" width="32" height="32" alt="">
<img src="https://favicons.page/color/selfhst/jellyfin/superellipse" width="40" height="40" alt="">
```

## Self-host in one command

```bash
curl -O https://raw.githubusercontent.com/peleda-app/favicons/main/docker-compose.yml
docker compose up -d
```

Two containers from a 9 MB image (on GHCR and as `favicons/favicons` on Docker Hub): the API, and a renderer with no network, a
read-only root and no capabilities. This page itself is built with the
`site` feature and ships with prerendered catalogs as `ghcr.io/peleda-app/favicons.page`; release binaries are on [GitHub Releases](https://github.com/peleda-app/favicons/releases),
and `cargo install favicons` builds the API. Built-in Let's Encrypt, local
disk or S3, GCS and Azure caches, blocklists and every other option are in
[docs/SELF-HOSTING.md](docs/SELF-HOSTING.md).

## For agents and automations

[`/llms.txt`](https://favicons.page/llms.txt) explains the API to language
models, [`/openapi.json`](https://favicons.page/openapi.json) describes it for
tools, and every response exposes its headers and timings to any origin.

## Documentation

- [How it works](docs/FEATURES.md): sources, tracing, caching, the request path
- [HTTP API](docs/API.md): statuses, headers, palettes, fallback reasons
- [Security model](docs/SECURITY.md): threats, sandbox, hardening checklist
- [Self-hosting](docs/SELF-HOSTING.md): Docker, binaries, TLS, small servers
- [Performance](docs/PERFORMANCE.md): wrk numbers and the k6 load test suite

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

[MIT](LICENSE). The banner shows logos of open-source self-hosted projects, each a trademark of its project ([list](docs/brand/LOGOS.md)); page icons by [Phosphor](https://phosphoricons.com) ([MIT](docs/brand/LICENSE-phosphor-icons)) and [Fluent Emoji](https://github.com/microsoft/fluentui-emoji) ([MIT](docs/brand/LICENSE-fluent-emoji)).
