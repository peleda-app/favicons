# brillout/awesome-react-components

## Status

Eligible now (no star, age or download threshold in the rules), with one hard condition that this draft already meets: every PR that adds an entry must also remove at least one "un-awesome" entry. Here we remove `react-open-doodles` (last push 2022-12-12, about 4 years ago).

Realistic caveats: the README's bar is subjective ("solves a real problem ... in a unique, beautiful, or exceptional way", "has recent code commits"). The repo is 3 days old with 2 stars, so a reviewer may hesitate; nothing in the rules forbids it. The maintainers merge in rare batches (the last batch was 2026-01-26) and 117 PRs are open, so expect a long wait.

## Target file and section

- File: `README.md` (branch `master`), hand-edited, no generator or linter.
- Heading path: `## UI Components` > `### Icons` (intro line `_Display icons / icon set / emojis_`).

`### Icons` fits better than `### Miscellaneous`: it is a component that displays icons. `### Integrations with Third Party Services` is the alternative if a reviewer sees it as an API wrapper.

## Entry

```markdown
- [favicons.page](https://github.com/peleda-app/favicons/tree/main/clients/js) - [demo](https://favicons.page/) - [docs](https://github.com/peleda-app/favicons/tree/main/clients/js#react) - Any website's icon as a clean, sandboxed SVG with light/dark badges, from a self-hostable API with no key.
```

## Insertion point

CONTRIBUTING requires alphabetical order. The Icons section is not sorted today (iconify-react, react-icons, react-open-doodles, react-icomoon, tabler-icons-react, Lucide), but `favicons.page` sorts before every existing entry, so it goes first. That position is correct whichever way the section is read. The same diff removes `react-open-doodles`.

```diff
 ### Icons
 
 _Display icons / icon set / emojis_
 
+- [favicons.page](https://github.com/peleda-app/favicons/tree/main/clients/js) - [demo](https://favicons.page/) - [docs](https://github.com/peleda-app/favicons/tree/main/clients/js#react) - Any website's icon as a clean, sandboxed SVG with light/dark badges, from a self-hostable API with no key.
 - [iconify-react](https://github.com/iconify/iconify-react) - Over 40k icons from 50+ icon sets, including all popular icon and emoji sets.
 - [react-icons](https://github.com/gorangajic/react-icons) - Svg react icons of popular icon packs using ES6 imports.
-- [react-open-doodles](https://github.com/lunahq/react-open-doodles) - Awesome free illustrations as react components.
 - [react-icomoon](https://github.com/aykutkardas/react-icomoon) - With react-icomoon you can easily use the icons you have selected or created in icomoon.
 - [tabler-icons-react](https://tabler-icons-react.vercel.app) - A set of over 450 free MIT-licensed high-quality SVG icons.
```

## PR title

```
Add favicons.page, remove react-open-doodles
```

(This follows the most common title pattern among recent PRs, for example "Add SheetXL Grid, remove deprecated Sematable" and "Add Keyline Icons, remove tabler-icons-react".)

## PR body

(empty)

There is no PR template (`.github/` holds only `workflows`), and CONTRIBUTING requires no fields in the body. A one-line reason for the removal is optional and common in recent PRs. If the maintainer asks for one: "react-open-doodles: no commits since 2022-12."

## Checklist compliance

| Rule (CONTRIBUTING / README) | How we meet it |
|---|---|
| Keep the alphabetic sort order | Inserted first: "favicons.page" sorts before "iconify-react" |
| Reference a component only once | No existing favicon entry; searching PRs for "favicon" returns nothing |
| Succinct, sufficient, without flair | One factual clause, no adjectives like "awesome", "blazing" |
| Don't mention "React" in the description | Description does not contain "React" |
| One line, no wrapping | Single line |
| Direct demo link if available | `[demo](https://favicons.page/)`, the page itself is the live playground |
| Separate docs link if available | `[docs](.../clients/js#react)`, the React section of the client README (distinct from the name link) |
| Format `- [Name](url) - [demo](url) - [docs](url) - Description.` | Matches the CONTRIBUTING example, with ` - ` separators and a trailing period |
| MUST remove at least one un-awesome component | Removes `react-open-doodles` (last push 2022-12-12, 83 stars) |
| "Has recent code commits" (README) | Repo active, release v1.3.0 |
| "Not super popular and well-known" (README) | 2 stars, so this is met |

## Notes

- Target repo: 48,947 stars, default branch `master`, last push / merge 2026-01-26, 117 open PRs. Merges come in occasional batches, so the review delay could be months.
- Removal choice: don't pick `iconify-react` or `tabler-icons-react`. Open PRs #645 and #679 already propose removing them, which would cause merge conflicts or duplicate removals. No open PR touches `react-open-doodles`.
- Link target: the entry links to `clients/js` inside the monorepo, not the repo root, so the first thing a reviewer sees is the React package. Pointing the name at the repo root is an alternative.
- Risk: a reviewer may see a 3-day-old, 2-star project as not yet "awesome", or may object that it depends on a hosted service. The self-hostable wording in the description answers the second point.
- Alternative section: `### Integrations with Third Party Services` (under `## Utilities`). It is unsorted and has no demo/docs links in its entries.
- Verified 2026-11-22 against live `master` (README lines 368-377): section, neighbours and react-open-doodles line match; no open PR mentions react-open-doodles or favicon; open PRs #524, #609 (iconify-react) and #679 (tabler-icons-react) touch the same section, so a rebase may be needed. The word "free" was dropped from the description: PROJECT FACTS state no key and no sign-up, not pricing.
- Do not open the PR without the maintainer's explicit go-ahead. Submitting it needs a fork, which the draft rules forbid here.
