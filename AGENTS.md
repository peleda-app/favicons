# Agent guide

Read [docs/RULES.md](docs/RULES.md) first: the rules of every repository of the workspace, synced from dojo. Then [README.md](README.md) and [docs/README.md](docs/README.md); the security model is in [docs/security/model.md](docs/security/model.md).

favicons is a privacy-first website icon service: it fetches a site's HTML, manifest and icons, turns them into a clean, sandboxed SVG with light and dark theme colours, and serves it over HTTP at favicons.page. Every fetched byte is treated as hostile: decoders run in a sandboxed worker and outbound requests are guarded against SSRF and read receipts. Open source under MIT, published on crates.io and as container images.

Status: in production at favicons.page as one Rust binary crate (`src/`), with a JavaScript client in `clients/js`. Run `mise run check` before considering a change done.

## Layout

- `src/`: the single binary crate (server, fetcher, render sandbox, catalogs)
- `fuzz/`: fuzz targets for every decoder of untrusted input; they compile `src/` modules through `fuzz/fuzz_targets/modules.rs` (see [docs/DECISIONS.md](docs/DECISIONS.md)); `mise run fuzz -- all 30` runs them
- `clients/js`: the npm client `favicons.page`
- `site/`: `robots.txt`, `llms.txt` and `sitemap.xml` embedded in the binary
- `catalogs/`, `stats/`, `compare/`, `loadtest/`: catalog builders, stats page, comparison and load scripts
- `docs/`: the documentation published at favicons.page/docs

## Rules for this repository

- Fetched HTML, manifests, SVG and raster icons are untrusted. They are parsed only inside the render worker (`src/sandbox.rs`): a separate process under rlimits, and under seccomp on Linux. The parent revalidates every worker response (`decode_vector`, `decode_page`, `decode_palette`) as if the worker were compromised.
- `robots.txt`, `sitemap.xml` and `llms.txt` in `site/` stay; they are part of the public service.
- A change to rendering output bumps `MONO_REVISION` in `src/render.rs` or `REVISION` in `src/palette.rs`, which the `sites` workflow uses to re-render the top sites.
