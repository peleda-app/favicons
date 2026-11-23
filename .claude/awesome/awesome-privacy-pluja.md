# pluja/awesome-privacy

## Status

**Blocked now.** Two rules from `misc/Contributing.md` ("What makes a good entry") and the automated PR check (`scripts/checks.ts`) are not met:

- **Age: at least 6 months.** `MIN_AGE_DAYS = 183`; the bot posts a `fail` finding ("only N days old (minimum is 6 months)") when the repo is younger. The repo was created 2026-11-19, first tag v0.1.1 on 2026-11-21. Earliest date the age check passes: **2027-05-21** (183 days after repo creation). Contributing says age counts from when the current code started; if favicons' code predates the repo (moved from another host / opened later), that has to be stated in the PR and backed by history, otherwise 2027-05-21 stands.
- **Real users.** "A fresh repository with a handful of stars is not there yet; come back when it is." `MIN_STARS = 100` (bot `warn`). Currently 2 stars. Needs visible adoption (stars, npm/crates.io/Docker pulls, outside issues/PRs) by the time of submission.

Precedent: PR #1289 "Add Favikit and ogkit (client-side favicon & OG tools)" was closed by the maintainer: "Both repositories are only a few days old, and I wait at least six months before listing a project". PR #1328 (Developer Tools) was closed as "too new". Submitting before 2027-05-21 will be closed.

Earliest realistic submission: **after 2027-05-21, once the repo has on the order of 100+ stars and a stable release history** (currently v1.3.0, which is fine).

## Target file and section

- File: `data/sections/developer-tools.yaml` (README.md is generated, must not be edited)
- Heading path in README: `## 🛠️ Developer & Self-hosting` > `### Developer Tools` (top-level entries, above `#### IDEs`)

Why this section: favicons is a developer-facing building block (an icon API and JS components) that replaces Google's favicon endpoint (`google.com/s2/favicons`, `t*.gstatic.com/faviconV2`), which hands every visitor's IP and page to Google. There is no "web APIs / CDN" section; `Fonts` is the closest thematic analogue (third-party asset hosts leaking visitor IPs) but it is scoped to typefaces only. `Design Tools` is organised as Adobe/Figma replacements and does not fit. Developer Tools has no restrictive subsections and its top-level list already holds server-side tools (CrowdSec). Category fit is acceptable but not perfect; the reviewer may ask for a different section or decline on fit.

## Entry

```yaml
    - name: favicons
      url: https://favicons.page
      repo: https://github.com/peleda-app/favicons
      description: Website icon API that returns any site's favicon as a sanitised SVG without logging visitor IP addresses, a replacement for Google's favicon service.
      tags:
        - self-host
        - no-account
```

`license` and `added` are left out on purpose (CI fills them). "Self-hostable" is not repeated in the description because the README already renders the `self-host` tag (`README_TAGS` in `scripts/labels.ts`). No `platforms` field (it is a service plus self-hosted server, not platform-limited).

Rendered in README (generated, for reference):

```
- **[favicons](https://favicons.page)** - Website icon API that returns any site's favicon as a sanitised SVG without logging visitor IP addresses, a replacement for Google's favicon service. <sub>self-host · MIT</sub>
```

## Insertion point

Entries are sorted by `byName` (`localeCompare`, `sensitivity: "base"`, case-insensitive), so `favicons` goes between `CrowdSec` and `Hetty` in `data/sections/developer-tools.yaml`:

```diff
       caveats:
         - website-trackers
       added: 2020-12-08
+    - name: favicons
+      url: https://favicons.page
+      repo: https://github.com/peleda-app/favicons
+      description: Website icon API that returns any site's favicon as a sanitised SVG without logging visitor IP addresses, a replacement for Google's favicon service.
+      tags:
+        - self-host
+        - no-account
     - name: Hetty
       url: https://hetty.xyz/
```

## PR title

```
Add favicons to Developer Tools
```

(Matches merged #1310 "Add AFFiNE to Notes and Tasks" and the template example "Adds Example to Cloud Storage".)

## PR body

The repository's PR template requires filled fields (relationship disclosure is marked "Required"; undisclosed self-submissions are closed). Minimal required content only:

```markdown
## What this adds or changes

Adds favicons to Developer Tools. It replaces Google's favicon service (google.com/s2/favicons).

## Your relationship to the project

I'm the author.

## Checklist

- [x] I edited a file in `data/sections/`, not `README.md` (it's generated)
- [x] The project meets [what makes a good entry](https://github.com/pluja/awesome-privacy/blob/main/misc/Contributing.md#what-makes-a-good-entry): public code, 6+ months old with a stable release, real users, private by default
- [x] The description is one plain sentence, with no marketing words
```

Do not tick the second checkbox before 2027-05-21 and real adoption; ticking it now would be false.

## Checklist compliance

| Rule (source) | How we meet it |
|---|---|
| Edit `data/sections/*.yaml`, not README (Contributing, template) | Met: entry goes into `developer-tools.yaml`. |
| One project per PR (template) | Met. |
| Public code, buildable, open license preferred (Contributing) | Met: MIT, Rust, public repo, `cargo install favicons`, Docker. |
| 6+ months of steady work, `MIN_AGE_DAYS = 183` (Contributing, checks.ts) | **Unmet.** Repo is 3 days old (created 2026-11-19). Earliest 2027-05-21. |
| Updated within the last year (`MAX_IDLE_DAYS = 365`) | Met. |
| Stable release (template) | Met: v1.3.0, static binaries on GitHub Releases, crates.io, npm, Docker Hub/GHCR. |
| Real user base, not "a handful of stars"; `MIN_STARS = 100` warn | **Unmet.** 2 stars. |
| Private by default: no account where none is needed | Met: no API key, no sign-up; `no-account` tag. |
| No telemetry that can't be turned off | Met as far as the project facts go (client IPs never logged). Bot also scans code for telemetry libraries. |
| Secure: no open critical vulnerabilities | Met as far as known; seccomp sandbox, allowlist SVG writer, no SSRF. |
| Transparent: who makes it, how it is funded | Partially: author identifiable via peleda-app org; funding is not stated anywhere in the project facts. Worth adding a line to the README before submitting. |
| Replaces something; say what (Contributing, template) | Met: Google's favicon service (s2/favicons). |
| Description one plain sentence, no marketing words, no license/platform list | Met: no "privacy-first"/"blazing" etc.; license left to CI. |
| Leave out `license`, `added` (Contributing) | Met. |
| Disclose relationship; undisclosed self-submissions are closed | Met in PR body ("I'm the author"). |
| Website trackers get a `website-trackers` caveat | favicons.page has no trackers per project facts; bot will verify. |
| Domain registration age (bot `info` if < 12 months) | Likely flagged as a recent domain; info only. |
| `npm ci && npm run validate` passes locally | Schema-valid: `name`, `url`, `repo`, `description`, `tags` with allowed values `self-host`, `no-account`. |
| Low-effort / generated submissions closed without review | Keep the PR short and hand-written; submit from a human account. |

## Notes

- Target repo: pluja/awesome-privacy, 20,348 stars, last push 2026-11-14 (merge of #1310). Not archived. PRs are reviewed in monthly batches.
- The maintainer declines most new suggestions (recent closed PRs #1301–#1328 are almost all rejections of new tools). Strict 6-month rule is enforced manually, not only by the bot.
- Risk: category fit. The list targets end users replacing mainstream services; an icon API is a niche developer component. If rejected on fit, alternatives: propose it under `Fonts` reworded as a broader "third-party web assets" idea (would need a new section and a `data/meta.yaml` change, unlikely to be accepted), or skip this list in favour of developer-oriented awesome lists (awesome-selfhosted, public-apis).
- Possible addition at submission time: an `avoid` item for Google's favicon endpoint in Developer Tools would need a documented harm with `tosdr`/`source` evidence; not included here, keep the PR to one entry.
- Strengthen before submitting: publish funding/ownership info, collect usage signals (npm downloads, Docker pulls), and keep regular releases between now and May 2027.
- Submitter check in `scripts/pr-report.ts`: the bot warns "opened pull requests to N other awesome lists in the last two days" when the author opened PRs to 3+ other `awesome-*` repos within 2 days, and "the GitHub account is N days old" for accounts under 30 days. Do not submit this in the same 2-day window as PRs to other awesome lists.
- Disclosure regex in `scripts/pr-report.ts` matches "I'm the author", so the body above yields "submitted by the project's owner (disclosed)".
- Nothing was forked, pushed, starred or commented on; this is a draft only.
