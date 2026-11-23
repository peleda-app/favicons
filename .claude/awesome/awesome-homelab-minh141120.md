# Minh141120/awesome-homelab

## Status

Eligible now. No star, age or download minimum. The rules are "free / open source", "maintained (commit in the last 12 months)", "disclose affiliation" and "one PR per addition". favicons meets all of them: MIT, last release v1.3.0. Small, new, single-author tools are accepted here (Homepad, iotaudit, unlisted, SolidPing were all merged in November 2026 from their authors, at the end of the section).

The main risk is category fit. This list is organised as a GitOps/home-ops repository, and there is no icons or dashboard-assets section. The closest place is 07 Observability, which is where the start-page and dashboard tools are listed (Homepage, Homepad). Every addition is reviewed by a human, so the maintainer could turn it down as off-topic.

## Target file and section

- File: `README.md` (AGENTS.md: "All content is in `README.md`")
- Section: `## 07 Observability` (`kubernetes/apps/observability/`), tool list, above `**In the wild**`
- Position: last tool entry, after SolidPing. Tools are ordered by how often home-ops repos use them, not alphabetically. New, rarely used tools go at the end, as Homepad and SolidPing did.

## Entry

```markdown
- [favicons](https://github.com/peleda-app/favicons) - Self-hostable service that returns any website's icon, or an icon from packs such as Dashboard Icons and selfh.st, as a sandboxed SVG for dashboard tiles.
```

Format: `- [Name](url) - Description.` (CONTRIBUTING.md, tool entry). The separator is a hyphen and the description starts with a capital and ends with a period. The link points to the canonical GitHub repo, like neighbours Homepad, SolidPing and iotaudit.

## Insertion point

README.md, current lines 249-251 and 253-255:

```diff
 - [iotaudit](https://github.com/emirmrkaljevic/iotaudit) - Local, consent-first audit of what smart home devices on your own network send, with per-device grades and HTML reports.
 - [Homepad](https://github.com/finish06/homepad) - Multi-user start page with live status from Gatus.
 - [SolidPing](https://github.com/fclairamb/solidping) - Uptime monitoring with status pages and heartbeat checks, packaged as an OCI Helm chart with an optional CloudNativePG database.
+- [favicons](https://github.com/peleda-app/favicons) - Self-hostable service that returns any website's icon, or an icon from packs such as Dashboard Icons and selfh.st, as a sandboxed SVG for dashboard tiles.
 
 **In the wild**
 
 - **onedr0p** — [o11y/](https://github.com/onedr0p/home-ops/tree/main/kubernetes/apps/o11y)
```

## PR title

```
readme: add favicons to 07 Observability
```

(AGENTS.md: "Commit messages use the prefix of the area they change: `readme: add ...`". Merged examples: "readme: add Homepad to 07 Observability", "readme: add SolidPing to 07 Observability".)

## PR body

The user's rule is that PR bodies stay empty. Here that rule can't be followed in full. AGENTS.md says to make "one pull request per addition, filled out with `.github/pull_request_template.md`", and CONTRIBUTING.md requires both an explanation of the placement and an affiliation disclosure. The body below fills in only the template's own fields, kept short and factual:

```markdown
**What are you adding?**

A tool: [favicons](https://github.com/peleda-app/favicons), in `07 Observability`, at the end of the tool list because it is new and not yet used in home-ops repos.

**Why does it belong in this list?**

It provides icons for the dashboards in this layer (Homepage, Homepad): a self-hosted, MIT-licensed service that returns a website's icon or an icon-pack icon (Dashboard Icons, selfh.st, Simple Icons) as a sanitized SVG. It ships as a Docker image for amd64 and arm64, with a Compose file.

- [x] I read [CONTRIBUTING.md](../CONTRIBUTING.md).
- [x] The entry follows the format for its type in CONTRIBUTING.md.
- [x] The resource is maintained.
- [x] I disclosed any affiliation.

Affiliation: I am the author of favicons.
```

## Checklist compliance

| Rule (source) | How we meet it |
| --- | --- |
| Tool format `- [Name](url) - Description.` (CONTRIBUTING) | Followed exactly, with a hyphen separator and a trailing period |
| Order by popularity, explain placement in PR (CONTRIBUTING, AGENTS) | Placed last, with the reason given in the PR body |
| Free resources only, open source (CONTRIBUTING) | MIT, free public instance, no API key or sign-up |
| Maintained: commit in the last 12 months (CONTRIBUTING) | Created 2026-11-19, v1.3.0 is current |
| Short, neutral description, capital first, period last, no marketing (CONTRIBUTING) | Factual description; words like "privacy-first" and "clean" left out |
| Canonical URL, no redirects (CONTRIBUTING) | `https://github.com/peleda-app/favicons`, repo not renamed |
| Disclose affiliation (CONTRIBUTING, template) | "Affiliation: I am the author of favicons." |
| One PR per addition (CONTRIBUTING, AGENTS) | Single-entry PR |
| PR filled with the template (AGENTS) | All template fields filled |
| Lychee link check (CI) | The only new link is a public GitHub repo |
| Human review, "Passing CI isn't enough" (CONTRIBUTING) | Unmet until reviewed: category fit is the open question |

## Notes

- Repo: 99 stars. Last commit and push 2026-11-20 (merged PR #22). Maintainer merges outside PRs within about 1-3 days. One open PR (#23, VirtoMir, 01 Infrastructure) touches a different section, so there is no conflict.
- Fit risk: the list is about GitOps layers. favicons is an asset service for dashboards, not monitoring. Homepage and Homepad sit in 07 because they are dashboards, and favicons rides on that. If it's rejected, the only other place is a row in `10 Apps`. That table requires a kubesearch.dev page and a real deployment, and favicons has neither, so it doesn't fit there now.
- Making the case stronger: if a home-ops repo starts using favicons, or if a Helm chart or Kubernetes manifest is published, a link to it could go in the PR. Nothing like that exists now, so none was claimed.
- The prior merged PR #15 (Homepad) said "prepared with AI assistance". Nothing requires that here, so it was left out.
- The description says "sandboxed SVG" (from the project facts) and avoids words like "privacy-first", in line with the "no marketing language" rule.
