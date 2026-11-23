# web-padawan/awesome-web-components

## Status

Eligible now. CONTRIBUTING.md has no star, age, download or human-only rule. The only fit test the maintainer applies in practice is that the project actually ships a custom element (PR #138 was closed with "There doesn't seem to be any custom elements in your project"). `favicons.page/element` registers a real `<favicon-icon>` custom element (`customElements.define`, open shadow root, `part="icon"`, observed attributes), so it passes. Small, young projects get merged (for example #130 phantom-ui, #139 `<aura-router>`).

## Target file and section

- File: `README.md` (no data file or generator, no linter config, no `.github/` directory)
- Section: `## Real World` > `### Components`

## Entry

```markdown
- [`<favicon-icon>`](https://github.com/peleda-app/favicons/tree/main/clients/js) - Web component that shows any website's icon as a sandboxed SVG from a self-hostable favicon service.
```

## Insertion point

Alphabetical by tag name: `<emoji-picker>` < `<favicon-icon>` < `<fg-modal>` ("fa" sorts before "fg").

```diff
 - [`<deep-chat>`](https://github.com/OvidijusParsiunas/deep-chat) - Web component for chat with AI capabilities.
 - [`<emoji-picker>`](https://github.com/nolanlawson/emoji-picker-element) - Lightweight emoji picker, distributed as a web component.
+- [`<favicon-icon>`](https://github.com/peleda-app/favicons/tree/main/clients/js) - Web component that shows any website's icon as a sandboxed SVG from a self-hostable favicon service.
 - [`<fg-modal>`](https://github.com/filamentgroup/fg-modal) - Accessible modal dialog web component.
 - [`<file-viewer>`](https://github.com/avipunes/file-viewer) - Web component built with Svelte to view files.
```

## PR title

```
Add `<favicon-icon>` to Components
```

(Matches merged titles such as "Add `<aura-router>`  to Components" (#139) and "Add phantom-ui to Components" (#130).)

## PR body

(empty)

There is no PR template and CONTRIBUTING.md requires no filled fields, so the body stays empty.

## Checklist compliance

| Rule (CONTRIBUTING.md) | How we meet it |
| --- | --- |
| Search previous suggestions first | Checked open and closed PRs: no favicon or `<favicon-icon>` entry or suggestion exists. |
| Format `[Name](Link) - Brief description` | Same format as the section: backticked tag name as the link text, ` - ` separator (hyphen, not the en dash one entry uses), trailing period. |
| Use title-casing | Applies to the name. In Components the name is the lowercase tag in backticks and descriptions are sentence case, so the entry copies its neighbours. |
| Drop `A`/`An` from the start of the description | The description starts with "Web component", like `<css-doodle>`, `<deep-chat>` and `<model-viewer>`. |
| Add to the relevant category in alphabetical order | Real World > Components, between `<emoji-picker>` and `<fg-modal>`. |
| Check spelling and grammar | Done. |
| Useful PR and commit title | `Add \`<favicon-icon>\` to Components` (the commit can use the same one line). |

## Notes

- Link target: most entries link to the element's own repo. Ours lives in a subfolder of the service repo, so the link points at `clients/js`, whose README documents `favicons.page/element` and `<favicon-icon>`. Another option is the npm page https://www.npmjs.com/package/favicons.page, but GitHub links are the norm in this section.
- Risk: the element depends on a network service (favicons.page by default, or your own instance via `base-url`). A reviewer could read it as promoting a service. The description says "self-hostable" so the element does not look tied to one vendor.
- Risk: the project is very new (repo created 2026-11-19, 2 stars). No rule forbids that, but other lists may care.
- Wrong sections: "Component Libraries" is for multi-component sets, and "Libraries > Integrations" is for framework glue. A single element belongs in Components.
- Maintainer activity: web-padawan reviews and merges personally. Recent merges: #151 (2026-10-12), #150 (2026-10-05), #136 (2026-10-09). The last push was 2026-10-15, and nothing has been merged since. 37 PRs are open, among them about ten Components additions (#152 to #170) with no review yet. Merge time varies a lot: #130 took minutes, #139 about two weeks, #136 nearly three months. Expect a wait.
- Repo: 3,637 stars, not archived, default branch `main`.
