# awesome-foss/awesome-sysadmin

## Status

**Blocked. Do not submit now.**

1. **First release must be more than 12 months old** (PR template checkbox and addition.md: "first released more than 12 months ago"). The first release tag v0.1.1 is dated 2026-11-21, so the earliest date that fits is **2027-11-22**.
2. **Rule against submitting your own software**: "Software you are submitting is not your own, unless you have a healthy ecosystem with a few contributors (which aren't your sock puppet accounts)." Today the project has 2 stars and is maintained only by the maintainer. A maintainer cannot check this box honestly until there are several independent contributors, or until a third-party user submits the project.
3. **Category fit is weak.** No tag matches an icon/favicon API. The only option is `Miscellaneous` ("Software that does not fit in another section."). The list is for "professional setup, not personal use", and a maintainer recently closed a PR with "This list is for production ready software." An icon service for dashboards and web apps is closer to awesome-selfhosted than to sysadmin tooling, so a curator may reject it even after the date.
4. **Machine/LLM-generated contributions** that do not follow the guidelines lead to a ban. The PR's free-text answers ("Have you used it? For how long?" etc.) must be written by a person from real use.

Conclusion: no earlier than 2027-11-22, and only once there are a few outside contributors (or someone else submits it). Even then, expect a category-fit objection.

## Target file and section

- **PRs are NOT accepted in `awesome-foss/awesome-sysadmin`.** Its PR template says: "Please do not submit pull requests in this repository. Use https://github.com/awesome-foss/awesome-sysadmin-data instead."
- Real target: `awesome-foss/awesome-sysadmin-data` (branch `master`). Add a **new file** `software/favicons.yml`.
- Tag/section: `tags/miscellaneous.yml`. In the generated Markdown list this is `# Awesome Sysadmin` > `## Software` > `### Miscellaneous`.

## Entry

New file `software/favicons.yml`. Field order follows `addition.md`, the same as the merged "Add Plakar" PR (#86). Existing files such as `fog.yml` show `source_code_url` after `tags` only because the metadata bot rewrites them; both orders were accepted (see "Add OpenBao", #104). No comments, no unused optional fields. `Rust`, `Docker` (platforms/), `MIT` (licenses.yml) and `Miscellaneous` (tags/) all exist in the data repo. Metadata fields (`stargazers_count`, `commit_history`, ...) are filled in by the bot, not by the submitter.

```yaml
name: favicons
website_url: https://favicons.page
source_code_url: https://github.com/peleda-app/favicons
description: Website icon service returning sandboxed, sanitized 128x128 SVG icons and light/dark theme colors over an HTTP API, with disk, S3, GCS or Azure cache.
licenses:
  - MIT
platforms:
  - Rust
  - Docker
tags:
  - Miscellaneous
demo_url: https://favicons.page
```

How it would render in the generated README (Miscellaneous section):

```markdown
- [favicons](https://favicons.page) - Website icon service returning sandboxed, sanitized 128x128 SVG icons and light/dark theme colors over an HTTP API, with disk, S3, GCS or Azure cache. ([Demo](https://favicons.page), [Source Code](https://github.com/peleda-app/favicons)) `MIT` `Rust/Docker`
```

Description length: 151 characters (limit 250), sentence case, no "open-source/free/self-hosted". The language is given because the project ships a single static binary.

## Insertion point

The data repo adds a new file, so there is no insertion inside it. The generated README sorts entries case-insensitively, so the entry would land between DadaMail and Fog in `### Miscellaneous` (current text of awesome-foss/awesome-sysadmin README.md):

```diff
 - [Chocolatey](https://chocolatey.org/) - The package manager for Windows. ([Source Code](https://github.com/chocolatey/choco)) `Apache-2.0` `C#/PowerShell`
 - [Clonezilla](https://clonezilla.org/) - Partition and disk imaging/cloning program. ([Source Code](https://clonezilla.org/downloads/src/)) `GPL-2.0` `Perl/Shell/Other`
 - [DadaMail](https://dadamailproject.com/) - Mailing List Manager, written in Perl. ([Source Code](https://sourceforge.net/projects/dadamail/files/)) `GPL-2.0` `Perl`
+- [favicons](https://favicons.page) - Website icon service returning sandboxed, sanitized 128x128 SVG icons and light/dark theme colors over an HTTP API, with disk, S3, GCS or Azure cache. ([Demo](https://favicons.page), [Source Code](https://github.com/peleda-app/favicons)) `MIT` `Rust/Docker`
 - [Fog](https://www.fogproject.org/) - Cloning/imaging solution/rescue suite. ([Source Code](https://github.com/FOGProject/fogproject)) `GPL-3.0` `PHP/Shell`
 - [phpList](https://www.phplist.org/) - Newsletter and email marketing software. ([Source Code](https://github.com/phpList/phplist3)) `AGPL-3.0` `PHP`
```

## PR title

CONTRIBUTING suggests the commit message `add My Awesome software`. Recently merged PRs use `Add OpenBao` and `Add Plakar`.

```
Add favicons
```

## PR body

The PR template **requires** you to keep its text and check the boxes ("DO NOT DELETE THE TEXT BELOW"; "Pull Requests not respecting the PR template will be rejected and the submitter banned"). So the body cannot be "(empty)". Below is the minimal factual fill. The free-text answers must be written **by a person** from real experience. They are left as the template's own placeholder lines on purpose, because generated answers break the LLM rule. CI (`make awesome_lint`, hecat) checks the YAML only, not the body.

Two boxes CANNOT be checked honestly today: "not your own" and "first released more than 12 months ago". This is the reason the PR is blocked.

```markdown
<!-- If you are adding new software to the list, DO NOT DELETE THE TEXT BELOW . Please make sure relevant boxes are checked [x] -->
<!-- If you are simply updating an existing entry or removing a project, DO delete the text below. -->

Thanks for taking the time to suggest an addition to awesome-sysadmin!

To ensure your Pull Request is dealt with swiftly, please check the following (check the boxes `[x]`):

- [x] Submit one item per pull request. This eases reviewing and speeds up inclusion.
- [x] Your addition is [Free software](https://en.wikipedia.org/wiki/Free_software).
- [x] You have searched the repository for any relevant [issues](https://github.com/awesome-foss/awesome-sysadmin-data/issues) or [PRs](https://github.com/awesome-foss/awesome-sysadmin-data/pulls), including closed ones.
- [x] Any software you are adding is not already listed at any of [awesome-selfhosted](https://github.com/awesome-selfhosted/awesome-selfhosted), [dbdb.io](https://dbdb.io/browse).
- [ ] Software you are submitting is not your own, unless you have a healthy ecosystem with a few contributors (which aren't your sock puppet accounts).
- [x] The file you are adding is formatted as described in [addition.md](https://github.com/awesome-foss/awesome-sysadmin-data/blob/master/.github/ISSUE_TEMPLATE/addition.md).
- [x] `Demo` links should only be used for interactive demos, i.e. not video demonstrations. If login credentials are required to access the demo, please link to the credentials directly.
- [x] Comments and unused optional fields have been removed.
- [x] The file you are adding uses [kebab-case](https://en.wikipedia.org/wiki/Letter_case#Kebab_case) file naming, for example `my-awesome-software.yml`.
- [x] Values for `platform` should match the platforms required to install and run the software.
- [x] Any software project you are adding to the list is actively maintained.
- [ ] Any software project you are adding was first released more than 12 months ago.
- [x] Any software project you are adding has working installation instructions.
- [x] You understand that your Pull Request will be merged at least ~1 week after approval, depending on maintainers time.

--------------

Please take some time to answer the following questions as best you can:

<!-- Failure to answer these questions in a useful and unbiased way will result in your submission being rejected. -->

- **Why is it awesome?**

Replace this text with your answer.

- **Have you used it? For how long?**

Replace this text with your answer.

- **Is this in a personal or professional setup?**

Replace this text with your answer.

- **How many devices/users/services/... do you manage with it?**

Replace this text with your answer.

- **Biggest pros/cons compared to other solutions?**

Replace this text with your answer.

- **Any other comments about your use case, things you've found excellent, limitations you've encountered... ?**

Replace this text with your answer or leave it blank.
```

## Checklist compliance

| Rule (source) | Status |
|---|---|
| PRs go to awesome-sysadmin-data, not awesome-sysadmin (awesome-sysadmin PR template) | Met: the target is `awesome-foss/awesome-sysadmin-data` |
| One item per PR | Met |
| Free software | Met: MIT, which is in `licenses.yml` |
| Searched issues/PRs | Checked: searching "favicon" in awesome-sysadmin-data gives 0 results |
| Not already in awesome-selfhosted / dbdb.io | Not in the awesome-selfhosted data repo (the code search for "favicon" only hits unrelated files: conf.py, dashbeam.yml, cheezypizza.yml). Check again before submitting |
| Not your own unless there is a healthy ecosystem with a few contributors | **Not met**: single maintainer, 2 stars |
| Format per addition.md, no comments or unused fields | Met: see the Entry section |
| kebab-case file name | Met: `favicons.yml` |
| Platforms match what is needed to install and run | Met: `Rust` (static binary / cargo install) and `Docker` (image favicons/favicons) |
| Description under 250 characters, sentence case, no redundant terms, short form | Met |
| Static binary: say the language | Met: `Rust` |
| Demo link must be interactive | Met: favicons.page is a live playground, no login |
| Actively maintained | Met: current release v1.3.0, recent commits |
| First released more than 12 months ago | **Not met**: v0.1.1 on 2026-11-21, so the earliest date is 2027-11-22 |
| Working installation instructions | Met: docs/SELF-HOSTING.md, docker-compose.yml, Releases, crates.io |
| Intended for professional setup | Arguable: it can serve infrastructure dashboards and internal tools, but it is not a classic sysadmin tool |
| Does not depend on a specific cloud provider | Met: local disk cache by default; S3/GCS/Azure are optional |
| Not a library/SDK that needs application code | Met for the server. Do not present the npm package (`favicons.page`) as the subject |
| Not a mere Dockerization or port | Met |
| No machine/LLM-generated contribution that breaks the guidelines | The submitter must write the free-text answers personally |
| Merge comes at least ~1 week after approval | Note only |

## Notes

- awesome-foss/awesome-sysadmin: 35,793 stars, last push 2026-10-22 (generated Markdown, legacy). awesome-foss/awesome-sysadmin-data: 17 stars, last push 2026-11-22. The maintainers are active: the last merges were PR #109 on 2026-10-21 and #86 "Add Plakar" on 2026-10-22, and many new additions stay unmerged or get closed (#106 was closed with "This list is for production ready software").
- The canonical rendering is the HTML version at https://sysadmin.awesome-selfhosted.net/. The Markdown README is generated by `hecat`.
- The awesome-sysadmin page tells you to check that the software is not in awesome-selfhosted, and that list is the more natural home for favicons (a web service), with the same 12-month-age rule. Do not submit to both. awesome-selfhosted is probably the better target after 2027-11-22, and awesome-sysadmin can be skipped.
- Risk: under the PR template rules, a submission that misses boxes or has a weak description can get you **banned**. Do not submit early to "hold a place".
- Before submitting, recheck the description against the current feature set and the current README of awesome-sysadmin-data. The rules and template may change by 2027.
