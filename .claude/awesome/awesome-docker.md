# veggiemonk/awesome-docker

## Status

**Blocked, permanently for this project as it is now. Do not submit.**

- The list's main rule is that a project must be *for Docker*, not just *use Docker*. The README says: "**The project has to be for Docker, not just using Docker.**" The PR template opens with "IF THE PROJECT JUST RUNS IN DOCKER, AUTOMATICALLY DENIED".
- CONTRIBUTING's test asks: "If you removed the Docker integration, would the project still have a reason to exist?" For favicons the answer is yes. It is a favicon/icon HTTP service that also ships as static binaries, `cargo install favicons` and an npm client. Docker is only how it gets delivered.
- CONTRIBUTING's sanity check is the sentence "This project exists to ____". For favicons it reads "...return any website's icon as a sanitized SVG with theme colours". None of the required words (Docker, container, image (in the OCI sense), registry, Dockerfile, Compose, Swarm, BuildKit, OCI) appear.
- The CONTRIBUTING table names this exact shape as a rejection: "Awesome 150 web apps deployable with `docker run`" -> "Belongs in `awesome-selfhosted`; Docker is incidental."
- Maturity is a second blocker. The repo was created 2026-11-19 and has 2 stars. In PR #1509 a maintainer turned down a very early repository and asked for "at least a short maintenance history". The first blocker cannot be fixed by waiting, though, so there is no earliest date to aim for.

Condition that would make it eligible: none, unless the project grows a feature that exists *because of* Docker. One example would be auto-discovering icons for running containers from Docker labels for dashboards. That would be a different product.

## Target file and section

- File: `README.md` (branch `master`). The list is plain Markdown, validated by the repo's Go CLI (`make lint`, `make check-pr`).
- No section fits. The nearest one by page shape (self-hosted web service) is `# Projects` > `## User Interfaces` > `### Web`. That section only holds Docker management UIs (Portainer, dockge, Arcane...), so favicons would be off-topic there too. `### Deployment & Platforms` is for PaaS/CaaS and does not fit either.

## Entry

Hypothetical only, shown for format. It follows the list's style: `- [Name](GitHub URL) - One sentence, starting with a verb, ending with a period.` There is no author handle and no badge.

```markdown
- [favicons](https://github.com/peleda-app/favicons) - Self-hostable service that returns any website's icon as a sanitized 128x128 SVG, shipped as a ~9 MB scratch Docker image with a no-network renderer in Compose.
```

## Insertion point

Hypothetical, in `### Web`. The section is sorted alphabetically, case-insensitive: "favicons" comes after "DockScope" and before "Komodo".

```diff
 - [dockge](https://github.com/louislam/dockge) - Easy-to-use and reactive self-hosted docker compose.yaml stack-oriented manager.
 - [DockScope](https://github.com/ManuelR-T/dockscope) - Visualizes Docker containers in a 3D dependency graph with live metrics, logs, and an in-browser terminal.
+- [favicons](https://github.com/peleda-app/favicons) - Self-hostable service that returns any website's icon as a sanitized 128x128 SVG, shipped as a ~9 MB scratch Docker image with a no-network renderer in Compose.
 - [Komodo](https://github.com/mbecker20/komodo) - A tool to build and deploy software on many servers.
 - [Portainer](https://github.com/portainer/portainer) - A lightweight management UI for managing your Docker hosts or Docker Swarm clusters.
```

## PR title

Follows the merged PR style ("Add groundcover to Observability", "Add prepare.sh Docker Course to Where to Start"):

```
Add favicons to Web
```

## PR body

The template requires a checkbox, the "This project exists to ____" sentence and "What changed and why". So the body cannot be "(empty)". Minimal required fill:

```markdown
- [x] I HAVE READ .github/CONTRIBUTING.md

This project exists to return any website's icon as a sanitized SVG; it ships as a Docker image and Compose stack.

What changed and why: adds favicons to User Interfaces > Web.
```

(Being honest, this sentence fails the template's own keyword check. The maintainers would close the PR.)

## Checklist compliance

| Rule | Status |
| --- | --- |
| Project is *for Docker* (PR template: "IF THE PROJECT JUST RUNS IN DOCKER, AUTOMATICALLY DENIED") | **Unmet.** Docker is just how it is delivered. |
| "This project exists to ____" contains Docker/container/image/registry/Dockerfile/Compose/Swarm/BuildKit/OCI | **Unmet.** |
| Description makes the Docker connection obvious | Only partly. It mentions the image and Compose, but the connection is packaging, not purpose. |
| One link per entry | Met |
| Prefer GitHub repo URL over marketing page | Met (github.com/peleda-app/favicons, not favicons.page) |
| Alphabetical within section, case-insensitive | Met (after DockScope, before Komodo) |
| One concise sentence, starting with a verb | Mostly met. Neighbouring entries often start with a noun, as this one does. |
| `:yen:` for paid services | Not applicable (free, MIT) |
| Active in the last 2 years / not archived | Met |
| No duplicate or redirect links | Met |
| PR focused, names target category | Met |
| Maintenance history (maintainer practice, PR #1509) | **Unmet now.** Repo is 3 days old, 2 stars. |
| `make lint` / `make check-pr` | Formatting would pass. Not run (no fork/clone by rule). |

## Notes

- Recommendation: skip awesome-docker. CONTRIBUTING itself sends this kind of project to awesome-selfhosted; submit there and to favicon/icon or web API lists instead.
- Maintainers do enforce the rule. Recent unmerged closures include DockNest (#1602, #1593), Minimal Containers (#1583) and DevOpsForge (#1509, early repo / no history).
- Recent merges are all about Docker itself: Docker learning resources (#1594, #1533, #1606), Docker observability (#1450, #1474) and nginx-lua (#1539).
- veggiemonk/awesome-docker: 37,285 stars. Last push 2026-11-18, last commit 2026-11-12, entry PRs merged as recently as 2026-11-04. Default branch `master`.
- Risk: a PR submitted anyway would likely be closed fast and could look like self-promotion spam from the peleda-app org.

## Verification (2026-11-22)

Checked against the live repo, read-only: the README rule (line 9), the PR template, CONTRIBUTING, the `### Web` section (README lines 417-431) and the neighbouring lines, the default branch `master`, and PRs #1509, #1450, #1594, #1602 and #1583. All quotes and claims match. One fix: "Self-hosted" became "Self-hostable" to match the project facts. The verdict stands: blocked, do not submit.
