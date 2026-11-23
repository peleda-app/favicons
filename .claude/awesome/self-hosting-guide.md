# mikeroyal/Self-Hosting-Guide

## Status

Eligible now by the written rules (no star, age or download thresholds; no PR template; no linter/CI). Practically blocked by inactivity: the last commit and the last merged PR are from 2025-06-27; 0 PRs merged since 2025-07-01 and 48 PRs are open, including recent "Add X" PRs (#343-#369) that are still unanswered. A PR can be prepared and opened any time, but expect it to sit unmerged until the maintainer does another batch-merge (historically he merges many PRs at once).

The "Make sure you've personally used or benefited from the suggested resource" rule is met (we are the authors), but it is a self-submission; nothing in CONTRIBUTING forbids that.

## Target file and section

- File: `README.md` (branch `main`, single 7k-line file, no data file/generator)
- Heading path: `# Getting Started with Self-Hosting` > `## Tools for Self-Hosting` > `### Development`
- Why: favicons is a developer-facing self-hosted API service (icon/badge API + npm client). The Development section already holds similar self-hostable dev utilities (IT-Tools, Coolify, Code-Server). There is no Icons/Images/APIs section; Dashboards is monitoring-only (Grafana, Prometheus).
- The Table of Contents does not need a change (no new section).

## Entry

```markdown
[favicons](https://github.com/peleda-app/favicons) is a privacy-first, self-hostable service that returns any website's icon as a clean, sandboxed 128x128 SVG with light/dark theme colors, plus ready-made badges and 17 icon packs. It needs no API key, ships as a ~9 MB Docker image, and has a public instance at [favicons.page](https://favicons.page).
```

Style copied from neighbours in the section (`[Name](url) is a ... .`, plain Markdown link, sentence ends with a period, inline links allowed, no badges, blank line between entries):

- `[IT-Tools](https://it-tools.tech/) is a collection of handy online tools for developers, with great UX.`
- `[Coolify](https://coolify.io/) is an open-source & self-hostable Heroku/Netlify alternative.`
- `[Zeal](https://zealdocs.org/) is an offline documentation browser for software developers inspired by [Dash](https://kapeli.com/dash).`

CONTRIBUTING asks for `[Resource Title](url link) — description.`, but almost no entry in the README uses the em dash; the in-section majority uses "is a", so the entry follows the section.

## Insertion point

The section is not alphabetical (entries are appended chronologically), so the entry goes at the end of `### Development`, after Zeal (Zeal is README.md line 430, followed by a blank line 431 and `### Web servers` on line 432, verified 2026-11-22):

```diff
 [krunvm](https://github.com/containers/krunvm) is a CLI-based utility for creating microVMs from OCI images, using [libkrun](https://github.com/containers/libkrun) and [buildah](https://github.com/containers/buildah).
 
 [Zeal](https://zealdocs.org/) is an offline documentation browser for software developers inspired by [Dash](https://kapeli.com/dash).
 
+[favicons](https://github.com/peleda-app/favicons) is a privacy-first, self-hostable service that returns any website's icon as a clean, sandboxed 128x128 SVG with light/dark theme colors, plus ready-made badges and 17 icon packs. It needs no API key, ships as a ~9 MB Docker image, and has a public instance at [favicons.page](https://favicons.page).
+
 ### Web servers
 
```

LF line endings, no trailing whitespace.

## PR title

```
Add favicons to Development
```

(Matches merged examples: "Add Gramax to Wikis Section", "Add OpenResty Manager", "Add Back4app Web Deployment Platform".)

## PR body

(empty)

No PR template exists and CONTRIBUTING has no required fields ("Expand on why the resource is useful in your pull request if needed" is optional).

## Checklist compliance

| Rule (CONTRIBUTING.md) | How we meet it |
|---|---|
| Search previous pull requests first | Checked: no open or closed PR mentions favicons/favicon APIs; README has no favicon/icon API entry. |
| Adding/improving categories welcome | Not adding a category; using existing `### Development`. |
| Personally used or benefited | Met (own project; self-submission, not forbidden). |
| One PR per suggestion | Single entry in a single PR. |
| Format `[Resource Title](url link) — description.` | Partially: link + description + trailing period; uses "is a" instead of the em dash to match the section's neighbours. Swap to ` — A privacy-first ...` if the maintainer prefers the literal format. |
| Short, simple, descriptive | Two sentences, comparable to neighbouring entries. |
| Spelling/grammar | Checked. Uses US "colors" to match the README (no "colour" anywhere in it); full sentences, no fragments. |

## Notes

- Verified 2026-11-22 against live `main`: CONTRIBUTING unchanged, no `.github/` directory (no PR template, no workflows), last commit 2025-06-27T01:51:15Z, last merge #320 at 2025-06-27, 48 open PRs, no PR mentioning favicon.

- Target repo: 23,633 stars; last push/commit 2025-06-27 (about 17 months ago); 48 open PRs; 0 merges since 2025-07-01. High inactivity risk; not archived.
- Maintainer merges in bursts (e.g. ~10 PRs within minutes on 2025-06-27), so a PR may get merged in a future burst, but there is no timeline.
- The README is long and messy (mixed "is a" / " - " entry styles, duplicate Gitea/Taiga entries); keep the diff to exactly the 2 added lines to avoid conflicts with the 48 open PRs.
- Alternative sections considered: `### Web servers` (reverse proxies only, wrong fit), `### Dashboards` (monitoring tools), `### Bookmarks` (apps, not APIs). Development remains the best fit.
