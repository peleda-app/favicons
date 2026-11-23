# notlmn/awesome-icons

## Status

**Eligible now on paper, weak fit in practice.** No written rule blocks submission: no star, age or download threshold, no human-only clause, no PR template. The list is linted with `awesome-lint` in CI (`.github/workflows/test.yml`, `npm test`).

The real gate is the maintainer's taste, and his recent closing comments matter more than the written rules:
- #39 (IcoGenie, closed 2026-02-10): "a tool to generate icons, not a predefined (which we can then curate) set of icons". favicons fetches arbitrary website icons, so it can be read the same way.
- #50/#52/#53 (closed 2026-08-27): "No paid / freemium stuff". favicons is free and MIT, so this is fine. Say it in the entry.
- #37 (FlyonUI): rejected for repackaging an icon set that is already listed. favicons bundles Simple Icons and Devicon, which are already in Logos. That is why the entry goes in Frameworks (aggregators), not in Logos.
- #38 (Unicon): the maintainer moved an aggregator to "Frameworks" himself, "so that separate actual icon sets vs aggregated sets like these".

Traction is minimal (repo created 2026-11-19, 2 stars, 3 days old). This is not a rule, but the list aims for an "aha" moment. If we want better odds, submit after the repo shows some adoption. Waiting is a recommendation, not a hard requirement.

## Target file and section

- File: `readme.md` (branch `master`)
- Heading path: `## Other` > `### Frameworks` ("Usually a library or framework that lets you import other icon sets.")

## Entry

```markdown
- [favicons](https://github.com/peleda-app/favicons#readme) - Free, self-hostable service that serves any website's icon or any icon from 17 bundled sets (Simple Icons, Devicon, Dashboard Icons and more) as a sanitized SVG, with React and Vue components and a Svelte action. ([Website](https://favicons.page))
```

Style copied from neighbours: `- [Name](repo#readme) - Description. ([Website](url))`. Repo links use `#readme`, the description ends with a period, and a lowercase project name is allowed (`css.gg`, `cssicon`, `icono`).

## Insertion point

Frameworks is alphabetical, case-insensitive (Iconbuddy, Iconify Deign, React Icons, Unicon). "favicons" sorts first.

```diff
 ### Frameworks
 
 Usually a library or framework that lets you import other icon sets.
 
+- [favicons](https://github.com/peleda-app/favicons#readme) - Free, self-hostable service that serves any website's icon or any icon from 17 bundled sets (Simple Icons, Devicon, Dashboard Icons and more) as a sanitized SVG, with React and Vue components and a Svelte action. ([Website](https://favicons.page))
 - Iconbuddy - A search engine to browse open source svg icons. ([Website](http://iconbuddy.com))
 - [Iconify Deign](https://github.com/iconify/iconify#readme) - Most popular icon sets in one framework containg over 200,000 open source vector icons. ([Website](https://iconify.design))
```

## PR title

```
Add favicons to the Frameworks section
```

This copies the accepted pattern ("Add GeoIcons to the Special section" #48, "Add Energy Icons to General" #60).

## PR body

(empty)

There is no PR template. contributing.md only suggests "Say why you're proposing the changes", which is a step in the web-editor flow and not a required field. Note: accepted PRs (#48, #60) had short bodies explaining the placement, and #38 got by with one line. An empty body is allowed but makes the PR slightly weaker.

## Checklist compliance

| Rule (source) | How we meet it |
|---|---|
| GitHub account (contributing.md) | Met. |
| "Awesome enough" / "aha" moment, awesome manifesto (contributing.md) | Arguable. It is a free, sanitized-SVG favicon plus icon-pack service with framework components. The maintainer may see it as a tool rather than a curated set (#39 precedent). Unproven traction (2 stars). |
| Edit `readme.md` only (contributing.md) | Met: one added line. |
| No paid / freemium (maintainer, commit 2026-08-27 "No paid stuff from now") | Met: MIT, no API key, no sign-up, no paid tier. "Free" is stated in the entry. |
| Don't duplicate already-listed sets (#37) | Mitigated by placing it in Frameworks next to the other aggregators (Iconify, Unicon). |
| Aggregators go in Frameworks (#38) | Met. |
| awesome-lint: `- [name](link) - Description.` format, description starts uppercase and ends with a period, no duplicate links | Met (entry follows the format exactly). Not run locally. |
| Alphabetical order within section (de facto) | Met: placed before Iconbuddy. |
| Repo link reachable (#39 rejected a private link) | Met: public repo. |

## Notes

- Target repo: 1,035 stars, default branch `master`, last push 2026-10-12 (merge of #60). The maintainer (notlmn) merges a few PRs a year and closes many submissions with one-line verdicts. Queue latency is weeks to months.
- Alternative section: `## Logos` ("Logos of different brands or companies or technologies"), between `Devicon` and `Simple icons`. It is a closer semantic match for "website icons", but it risks the #37 duplicate objection because Simple Icons and Devicon are listed there and favicons also serves them.
- Main risk: closed as "a tool/service, not an icon set" (the subtitle reads "downloadable SVG/PNG/Font icon projects"). The npm clients (`favicons.page/react` and `/vue` components, the `/svelte` action) are the strongest argument for the Frameworks fit.
- Do not open the PR from automation. The facts used are limited to the project brief.
