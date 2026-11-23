# TheComputerM/awesome-svelte

## Status

Eligible now. CONTRIBUTING.md sets no minimum for stars, age or downloads, and says nothing against new projects or maintainers adding their own work. Recent merged PRs include brand-new, self-submitted projects (#240 PR Cockpit, #224 File Viewer, #232 crd-ui). Soft risks: merges come in batches (a burst on 2026-09-06, then a single merge, #186, on 2026-11-03; 76 PRs open), so review may take weeks. The maintainer also screens category fit: #247 (OmniGet) was closed on 2026-11-12 as "not really a Svelte component". Keep the entry and body focused on the Svelte action (`favicons.page/svelte`), not on the service.

## Target file and section

`README.md` -> `## UI Components` -> `### Icons`

## Entry

```markdown
- [favicons.page](https://github.com/peleda-app/favicons/tree/main/clients/js) - A `use:favicon` action that shows any website's icon as a sandboxed SVG, served by a privacy-first, self-hostable favicon service.
```

## Insertion point

The section is not alphabetical; new entries are appended at the end (as with #190 @thesvg/svelte and #181 hugeicons). Insert after `moving icons`, before the blank line and `### Calendar`.

```diff
 - [hugeicons](https://github.com/hugeicons/svelte) - Beautiful, production-ready icon package for Svelte with complete icon coverage.
 - [moving icons](https://github.com/jis3r/icons) - A collection of beautifully crafted, animated Lucide icons.
+- [favicons.page](https://github.com/peleda-app/favicons/tree/main/clients/js) - A `use:favicon` action that shows any website's icon as a sandboxed SVG, served by a privacy-first, self-hostable favicon service.
 
 ### Calendar
```

## PR title

```
Add favicons.page
```

Commit message (CONTRIBUTING step 3 sets the format): `Add favicons.page`

## PR body

The body cannot be empty here: CONTRIBUTING step 5 says to describe in at least one sentence why the project is awesome. Minimal:

```
Svelte action from favicons.page that turns any domain into its icon (a 128x128 SVG rebuilt in a sandbox), with no API key and a self-hostable server.
```

## Checklist compliance

| CONTRIBUTING rule | How we meet it |
|---|---|
| 1. Fork the repo | Left to the maintainer (we do not fork). |
| 2. Add to `README.md` in the most fitting section | `UI Components > Icons`; neighbours are Svelte icon packages. |
| 2. Description starts uppercase, ends with a period | "A `use:favicon` action ... service." |
| 2. Format `- [Project Name](Project URL) - Sentence.` | Copied exactly: hyphen bullet, ` - ` separator, inline link. |
| 3. Commit as `Add {project name}` | `Add favicons.page`. |
| 4. Open an issue if no fitting field | Not needed, Icons fits. |
| 5. At least one sentence on why it is awesome | One sentence in the PR body. |
| 6. Make a PR | Left to the maintainer. |

No PR template, no `.github` directory, no linter/CI in the repo.

## Notes

- Repo: 2180 stars, last push / merge 2026-11-03 (#186), 76 open PRs; maintainer last active 2026-11-12 (closed #247). Maintainer merges in bursts; several valid PRs sit unmerged for months.
- Link target: the `clients/js` folder holds the README with the Svelte section (`use:favicon`, SvelteKit note). The npm page https://www.npmjs.com/package/favicons.page or https://favicons.page are alternatives; GitHub links dominate the section.
- Name choice: neighbours use the package name (`@thesvg/svelte`, `lucide-svelte`), so `favicons.page` (the npm package) rather than "favicons".
- Category risk: the section is icon sets; this is a favicon fetcher. If the maintainer objects, fallback is `UI Components > Miscellaneous` (append after `crd-ui`) with the same entry.
- Overlap: theSVG (@thesvg/svelte) is already listed and is one of the 17 packs favicons.page serves; do not mention it in the PR.
- Only facts from the project brief are used; the Svelte action and its options were checked in `clients/js/README.md` and `clients/js/svelte.js`, and the published npm package (0.1.1) exports `./svelte`.
- The action itself does not expose the light/dark palette, so the body no longer mentions colours.
