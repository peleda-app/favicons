# rust-unofficial/awesome-rust

## Status

**Blocked (not eligible now).** CONTRIBUTING's TL;DR says `(stars > 50 | downloads > 2000)`; CI enforces it as at least 50 stars or at least 2000 crates.io downloads (`stars >= required` / `downloads >= MINIMUM`) (`src/main.rs`: `MINIMUM_GITHUB_STARS = 50`, `MINIMUM_CARGO_DOWNLOADS = 2000`, checked on every PR and re-checked periodically).

Checked 2026-11-22:
- GitHub stars on peleda-app/favicons: **2** (need at least 50)
- crates.io downloads of `favicons`: **66** total (need at least 2000; crate created 2026-11-21, 7 versions, default 1.3.0)

No fixed date can be given. Submit once **either** condition holds: GitHub stars >= 50, **or** crates.io total downloads >= 2000 (CI's comparison). No age rule exists. CONTRIBUTING also allows "an equivalent level of other popularity metrics (which should be specified in the PR)", but CI checks only stars/downloads, so do not rely on Docker Hub pulls or npm downloads.

What to watch:
- `gh api repos/peleda-app/favicons --jq .stargazers_count`
- `curl -s -A "generic-client/1.0" https://crates.io/api/v1/crates/favicons | jq .crate.downloads`

## Target file and section

`README.md`, heading path: `## Applications` > `### Web`

Picked `### Web` over `### Web Servers`: Web Servers holds general-purpose HTTP servers, proxies and static file servers. Web holds self-hosted web services and applications (hook0, rss-funnel, Chhoto URL, Redlib, pagesift), which fits a favicon API service.

## Entry

```markdown
* [peleda-app/favicons](https://github.com/peleda-app/favicons) [[favicons](https://crates.io/crates/favicons)] - Privacy-first, self-hostable favicon API that returns any website's icon as a sandboxed 128x128 SVG, with light/dark theme colours in response headers. [![CI](https://github.com/peleda-app/favicons/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/peleda-app/favicons/actions/workflows/ci.yml)
```

## Insertion point

Alphabetical by link text, ignoring case: `MASQ-Project/Node` < `peleda-app/favicons` < `Plume-org/Plume` (`pe` < `pl`).

```diff
 * [LemmyNet/lemmy](https://github.com/LemmyNet/lemmy) - A link aggregator / reddit clone for the fediverse [![Build Status](https://cloud.drone.io/api/badges/LemmyNet/lemmy/status.svg)](https://cloud.drone.io/LemmyNet/lemmy)
 * [MASQ-Project/Node](https://github.com/MASQ-Project/Node) - MASQ Node software provides a decentralized mesh-network of nodes for global users to access normal internet content - next evolution of tech beyond Tor & VPN [![build badge](https://github.com/MASQ-Project/Node/actions/workflows/ci-matrix.yml/badge.svg)](https://github.com/MASQ-Project/Node/actions)
+* [peleda-app/favicons](https://github.com/peleda-app/favicons) [[favicons](https://crates.io/crates/favicons)] - Privacy-first, self-hostable favicon API that returns any website's icon as a sandboxed 128x128 SVG, with light/dark theme colours in response headers. [![CI](https://github.com/peleda-app/favicons/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/peleda-app/favicons/actions/workflows/ci.yml)
 * [Plume-org/Plume](https://github.com/Plume-org/Plume) - ActivityPub federating blogging application
 * [Redlib](https://github.com/redlib-org/redlib) - An alternative private front-end to Reddit, with its origins in [Libreddit](https://github.com/libreddit/libreddit)
```

(README.md lines 992-995 on main as of 2026-11-22.)

## PR title

```
Add peleda-app/favicons to Web
```

(Matches recent merged titles: "Add scalo to Configuration", "Add libracoder/kiri to Virtualization", "Add dataviz to Data visualization".)

## PR body

The repo's `.github/pull_request_template.md` contains one checkbox and nothing else. It is the only thing filled in:

```markdown
- [x] - I have read the [CONTRIBUTING.md](CONTRIBUTING.md) and my pull request fulfills the criteria therein
```

Tick it only once the stars or downloads threshold is actually met. Otherwise the statement is false.

## Checklist compliance

| Rule (CONTRIBUTING / template / CI) | How we meet it |
|---|---|
| stars >= 50 or crates.io downloads >= 2000 (CI-enforced) | **Unmet**: 2 stars, 66 downloads |
| Template `[ACCOUNT/REPO](https://github.com/ACCOUNT/REPO) [[CRATE](https://crates.io/crates/CRATE)] - DESCRIPTION` | Followed exactly; the crate is published, so the `[[favicons](...)]` part stays |
| CI badge after the description, separated by a space, with branch info | GitHub Actions `ci.yml` badge with `?branch=main` (workflow runs on push to main) |
| Alphabetical order (CI sorts list items by lowercased text) | Between `MASQ-Project/Node` and `Plume-org/Plume` |
| Rust project (CI checks Rust share of repo languages, min 1%) | Rust repository, met |
| Valuable / stable and useful to many users | Arguable: v1.3.0, public instance, Docker image, crates.io; popularity is the open point |
| markdownlint (`.markdownlint.json`: line length 1000, `*` bullets) | Single line, about 390 chars, `*` bullet |
| PR template checkbox | Filled (see PR body), but only true once the threshold is met |

## Notes

- rust-unofficial/awesome-rust: 60,729 stars; last push 2026-11-22 (very active; entry PRs merged daily, e.g. 2026-11-18 to 2026-11-21).
- CI also re-checks every listed link periodically, and a repo that falls below the threshold or disappears can fail the build. Expect a PR below the threshold to be rejected by CI.
- Fastest route is probably crates.io downloads >= 2000 rather than 50 stars. Do not inflate either artificially; maintainers look at this.
- Alternative section: `### Web Servers` (`biandratti/huginn-proxy` ... `vproxy/0x676e67`). Rejected because the project is an API service, not a general HTTP server.
- Do not open the PR now. Recheck when the threshold is met, rebase the insertion point against current `main` (the Web section changes often), then open it.
