# marcelscruz/public-apis

## Status

Eligible now. CONTRIBUTING.md has no star, age or download threshold. favicons meets every "What we accept" criterion: public, documented (API.md, OpenAPI, llms.txt), self-serve with no key or sign-up, custom domain (favicons.page), clean https URL, main product. Verified live on 2026-11-22: `https://favicons.page` returns 200 and `GET /color/github.com` returns `image/svg+xml` with `access-control-allow-origin: *`.

Main risk: an automated reviewer and the maintainer may treat a 3-day-old project with 2 stars as "low-effort" or self-promotion. The entry is factual and the service is live, so it should pass, but this is not guaranteed.

## Target file and section

- File: `README.md` (CONTRIBUTING: the `/db` folder is auto-generated, do not edit it)
- Section: `### Art & Design` (where `Favicondl` and `Icon Horse` are listed)

## Entry

```
| [favicons](https://favicons.page) | Any website's icon as a sanitized 128x128 SVG, with light/dark theme colors in response headers, plus 17 icon packs. Open source and self-hostable | No | Yes |
```

Description length: 146 characters (limit 160). Name without TLD, not ending in "API", lowercase like the brand (precedents in the same file: `eeemoji`, `pagespool`; sorting is case-insensitive).

## Insertion point

Alphabetical, case-insensitive: `Favicondl` < `favicons` (d < s) < `Folkroom`.

```diff
 | [EmojiHub](https://github.com/cheatsnake/emojihub) | Get emojis by categories and groups | No | Yes |
 | [Europeana](https://pro.europeana.eu/resources/apis/search) | European Museum and Galleries content | `apiKey` | Unknown |
 | [Favicondl](https://favicondl.com) | Get any website's favicon by URL (SVG/PNG/ICO), with JSON metadata and fallbacks | No | Yes |
+| [favicons](https://favicons.page) | Any website's icon as a sanitized 128x128 SVG, with light/dark theme colors in response headers, plus 17 icon packs. Open source and self-hostable | No | Yes |
 | [Folkroom](https://folkroom.org/data) | Open-access cultural heritage REST API for living folk traditions, instruments and UNESCO records | No | Yes |
 | [GetGenAI (Packaging Compliance Review)](https://www.getgen.ai/build) | Compliance review API for product labels and marketing assets — automated checks against regulations, retailer specs, and brand guidelines, always up to date | `apiKey` | No |
```

## PR title

```
Add favicons API
```

Commit message (single squashed commit, per CONTRIBUTING):

```
Add favicons API to Art & Design
```

## PR body

(empty)

The `.github/PULL_REQUEST_TEMPLATE.md` is an unenforced checklist; recently merged PRs (#1853, #1849, #1845) replaced it with free text, so nothing is required. Optional, only if the user decides to relax the empty-body rule for this repo (the automated reviewer checks Auth/CORS, so stating them can help):

```
Docs: https://github.com/peleda-app/favicons/blob/main/docs/API.md
OpenAPI: https://favicons.page/openapi.json
Auth: No · CORS: Yes (Access-Control-Allow-Origin: *)
```

## Checklist compliance

| Rule | How we meet it |
|---|---|
| Any use case / exposes a connectable API | `GET /color/{domain}`, `/mono/{domain}`, badges, icon packs |
| Free or paid, self-serve | Free, no key, no sign-up, first call works straight from the docs |
| Publicly reachable and documented, Auth/CORS determinable | Live; API.md + OpenAPI; CORS `*` verified with curl |
| Main product only | favicons is its own product |
| Custom domain | favicons.page |
| Clean URL, https, homepage not deep link | `https://favicons.page` |
| Quality bar | Live service, MIT source, releases, Docker image, npm package (subjective, see Status) |
| Not an app/library-only | It is an HTTP API; the npm client is a bonus, not the submission |
| Alphabetical order | Between Favicondl and Folkroom |
| One space padding per column | Yes |
| Auth field value from allowed list | `No` |
| CORS field value from allowed list | `Yes` |
| Name without TLD, not ending in "API" | `favicons` |
| Description under 160 chars | 146 |
| One link per PR | Yes |
| PR title `Add Api-name API` | `Add favicons API` |
| Not a duplicate / searched PRs and issues | `gh search prs favicon` finds no favicons.page submission; Favicondl and Icon Horse are different services |
| Squash into one commit, target `main` | To do when opening |
| Do not edit `/db` | Only README.md changes |
| Category with 3+ items | Existing category |

## Notes

- Repo: 9,762 stars, last push 2026-11-22 15:22 UTC; very active, several entry PRs merged per day.
- Submissions are reviewed by an automated bot first (may comment, approve or close), then the maintainer merges.
- Alternative section: `### Development` (many developer utilities), but CONTRIBUTING says to pick the section closest to the service, and both existing favicon APIs live in Art & Design, so that is the natural fit.
- CONTRIBUTING also points developer tools to marcelscruz/dev-resources; favicons could be listed there too (the npm client/self-hosted server), separately, without being a duplicate.
- Capitalisation choice: `favicons` matches the brand; `Favicons` would also be acceptable if the reviewer prefers it.
