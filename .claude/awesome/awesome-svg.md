# willianjusten/awesome-svg

## Status

Eligible now. `contributing.md` sets no star, age, download or human-only rule. The only automated check is `awesome_bot` (link checker, `ci_script.sh`) over `topics/*.md`, and Travis CI is effectively dead. The repo has had no merge since 2026-07-16 and 86 open PRs are waiting, so expect a slow review or none at all.

## Target file and section

File: `topics/Icons.md` (default branch `master`)
Heading path: `Awesome SVG` (Readme.md TOC) > `Icons` > `## Icons` ("How to use svg for icons, why svg is better than fonticon, tools, and more.")

Why: this section already lists logo and icon services with an API or search (theSVG "with ... a free API", LogoSear.ch, VectorLogoZone, IconSear.ch). `topics/Downloads.md` (SVG to Download) is the fallback.

## Entry

```markdown
* [favicons.page](https://favicons.page) - Self-hostable, privacy-first API that returns any website's icon as a sanitized 128x128 SVG, plus light/dark theme colours and icons from 17 packs by slug.
```

## Insertion point

The list is mostly alphabetical and ignores case. PR #203 put GeoIcons between Fontastic and Glyph. "favicons.page" sorts before "Feather" ("fa" < "fe").

```diff
 * [Create an icon font illustrator icomoon](http://www.sitepoint.com/create-an-icon-font-illustrator-icomoon/)
 * [Customizable svg icons css variables](http://codepen.io/AmeliaBR/post/customizable-svg-icons-css-variables)
+* [favicons.page](https://favicons.page) - Self-hostable, privacy-first API that returns any website's icon as a sanitized 128x128 SVG, plus light/dark theme colours and icons from 17 packs by slug.
 * [Feather](https://feathericons.com)
 * [Fontastic](http://fontastic.me/faq)
```

## PR title

```
Add favicons.page to Icons section
```

(Merged examples: "Add theSVG to Icons section" #186, "Add IcoGenie to Icons section" #184, "Add GeoIcons to the Icons list" #203. The maintainer squashes them into `docs: add ...` commits himself.)

## PR body

(empty)

Neither a PR template nor CONTRIBUTING requires a filled description. Merged PRs #178, #179 and #180 had empty bodies.

## Checklist compliance

- "Create a file section if needed": not needed, because the existing `topics/Icons.md` fits.
- "Search previous suggestions before making a new one": met. favicons and favicons.page are absent from all topic files, from open PRs and from closed PRs.
- "Add your links: `* [link-name](http://example.com/) - description`": met. The entry uses `* ` + `[name](url)` + ` - ` + description.
- "If your link isn't in english, use (Lang)": not applicable, because the site is in English.
- "Check your spelling and grammar": done. The text uses "colours" (British spelling, as on the project). Change it to "colors" if American spelling is preferred.
- "Remove trailing whitespace": the entry has none. Keep LF line endings (`.editorconfig`).
- "Send a Pull Request": prepared, not opened.
- awesome_bot link check: https://favicons.page must answer 2xx (redirects are allowed).

## Notes

- Repo stats: 4,665 stars. Last push and merge on 2026-07-16 (PR #205). 86 open PRs, many of them image-to-SVG tools that stay unmerged. The maintainer merges in batches, months apart.
- Link text: we use "favicons.page", not "favicons", so the entry is not confused with the well-known `favicons` npm favicon generator. It also matches the npm package name.
- Neighbouring entries differ on trailing periods (GeoIcons has one, IcoGenie and theSVG do not). We leave the period off, like most entries with a description.
- Alternative: `topics/Downloads.md` (SVG to Download), between "Freepik" and "Loading.io". The Icons section fits better because of theSVG and VectorLogoZone.
- The project is small (2 stars, created 2026-11-19). No rule here blocks that, but the maintainer may take it as self-promotion. Keep the entry factual.
- An open PR must not have a broken link. Make sure favicons.page stays up while the PR is open.
