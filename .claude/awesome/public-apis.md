# public-apis/public-apis

## Status

**Eligible now.** public-apis has no star, age, or download threshold. Its rules are: the API must be free (or have a free tier), must have proper documentation, and each PR adds one link. favicons.page meets all three: no key, no sign-up, free public instance, documented in docs/API.md and OpenAPI. CORS and HTTPS were checked live on 2026-11-22: `GET https://favicons.page/color/github.com` returns 200 `image/svg+xml` with `access-control-allow-origin: *`.

## Target file and section

- File: `README.md` (branch `master`). The list is hand-edited, with no separate data file.
- Section: `## Index` -> `### Art & Design`. This is where the existing favicon APIs sit (Favicondl, Icon Horse). Development holds link-preview services like unfurlr, not icon APIs.

## Entry

```
| [favicons](https://github.com/peleda-app/favicons/blob/main/docs/API.md) | Any website's icon as a sandboxed 128x128 SVG with light/dark theme colors, plus 17 icon packs | No | Yes | Yes |
```

- Description: 94 characters (the limit is 100), with no trailing punctuation.
- Name: `favicons`, not `favicons.page`, because the rules forbid the TLD in the name, and the name must not end in "API". Lowercase names are already accepted in this section (`pic2svg`, `raFont`).
- Link: the API documentation, as the rules require. Live alternative: `https://favicons.page`.
- No Postman column. CONTRIBUTING mentions one, but no row in the README uses it. All rows have 5 columns.

## Insertion point

The validator sorts by the uppercased name: `FAVICONDL` < `FAVICONS` < `HARVARD ART MUSEUMS`. That puts the new row right after Favicondl, so the new row becomes README.md line 312 (Favicondl is line 311 and Harvard Art Museums moves from 312 to 313). Re-checked against master on 2026-11-22; the last push was 2026-11-20T19:42Z.

```diff
 | [Face Shape Guide Lookup](https://myfaceshapechart.com/openapi.json) | Read-only lookup for published face-shape guide records | No | Yes | Yes |
 | [Favicondl](https://favicondl.com/documentation.html) | Get any website's favicon by URL (SVG/PNG/ICO), with JSON metadata and fallbacks | No | Yes | Yes |
+| [favicons](https://github.com/peleda-app/favicons/blob/main/docs/API.md) | Any website's icon as a sandboxed 128x128 SVG with light/dark theme colors, plus 17 icon packs | No | Yes | Yes |
 | [Harvard Art Museums](https://github.com/harvardartmuseums/api-docs) | Art | `apiKey` | No | Unknown |
 | [Icon Horse](https://icon.horse) | Favicons for any website, with fallbacks | No | Yes | Yes |
```

## PR title

```
Add favicons API
```

This follows CONTRIBUTING (`Add Api-name API`) and the recently merged PRs ("Add Lineup Atlas API", "Add bluerate API"). The commit message should be one line: `Add favicons API to Art & Design`.

## PR body

```
(empty)
```

The repo has a PR template, `.github/PULL_REQUEST_TEMPLATE.md`, but it is a self-check list, not a set of required fields, and no CI job reads the PR body. Merged PRs show it is optional: #8899, #8891 and #8886 were merged with no checklist at all, while #8904, #8894, #8890 and #8882 kept it. Under the rule that PR descriptions stay empty, the body is left empty.

If you do want to include the checklist anyway, this is the factual version (the "category" box stays unchecked because no new category is created, as in merged PR #8904):

```
- [x] My submission is formatted according to the guidelines in the [contributing guide](/CONTRIBUTING.md)
- [x] My addition is ordered alphabetically
- [x] My submission has a useful description
- [x] The description does not have more than 100 characters
- [x] The description does not end with punctuation
- [x] Each table column is padded with one space on either side
- [x] I have searched the repository for any relevant issues or pull requests
- [ ] Any category I am creating has the minimum requirement of 3 items
- [x] All changes have been [squashed][squash-link] into a single commit

[squash-link]: <https://github.com/todotxt/todo.txt-android/wiki/Squash-All-Commits-Related-to-a-Single-Issue-into-a-Single-Commit>
```

## Checklist compliance

| Rule (CONTRIBUTING / template / validator) | How we meet it |
|---|---|
| Free access or free tier; not marketing a paid product | Free public instance, MIT-licensed, self-hostable; no paid tier |
| Proper documentation | docs/API.md, plus https://favicons.page/openapi.json |
| Auth value from the allowed set | `No` |
| HTTPS / CORS values | `Yes` / `Yes` (`Access-Control-Allow-Origin: *`, checked live) |
| Alphabetical order (validator compares uppercased titles) | Placed between Favicondl and Harvard Art Museums |
| One space padding per column | Yes |
| Description <= 100 chars, no trailing punctuation | 94 chars, ends with "packs" |
| Name has no TLD and does not end with "API" | `favicons` |
| One link per PR | One row |
| Most fitting category | Art & Design, next to Favicondl and Icon Horse |
| Search for duplicates first | No favicons.page PR or issue found. Related open PR: #8922 (Logoapi Favicons, same section, different service) |
| Squash into one commit, target `master` | One commit on a fork branch, PR against `master` |
| Link validation CI on added lines | The docs URL returns HTTP 200 |
| `format.py` validator | Re-ran on 2026-11-22 against current master and the modified README: the error set is identical apart from line numbers, so the new row adds no errors. Master already fails the checker on unrelated rows, including Art & Design ordering because of "Text-till-Kladdesign", so CI may show red no matter what. |

## Notes

- Repo stats: about 500.8k stars. Last push 2026-11-20T19:42Z. Maintainers merged a batch of entry PRs that day (#8877 to #8904), so the repo is active.
- Open PR #8922 (Logoapi Favicons, opened 2026-11-10) also edits Art & Design. Its row goes near Lordicon, not near our line, so a merge conflict is unlikely.
- CONTRIBUTING warns that PRs judged to be marketing are closed. Keep the PR minimal and do not add a description that reads like an advertisement.
- The free-text paragraph that many merged PRs add was left out because of the rule that PR descriptions stay empty. If a maintainer asks for context, a one-line reply is enough: "Free, keyless favicon API; docs and OpenAPI linked."
- Alternative link target: `https://favicons.page` (the live demo). Docs are preferred because the guideline asks for documentation.
- Alternative section: Development. Rejected, because favicon APIs already live in Art & Design.
- There is no MCP Servers entry, since favicons is not an MCP server.
