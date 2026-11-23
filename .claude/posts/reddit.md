# r/SideProject

**Type:** video post + text. Video: `.claude/posts/favicons-slideshow.mp4` (16.8 s, 1600×1000, H.264). It is a slideshow of these banners, in order:

1. `~/Desktop/favicons-showcase.png`: "One URL, every kind of icon"
2. `~/Desktop/favicons-before-after.png`: dashboard before / after
3. `~/Desktop/favicons-homepage.png`: Homepage before / after
4. `~/Desktop/favicons-glance.png`: Glance before / after

**Title:**

I built a free favicon API that turns any site's icon into a clean SVG that looks right in light and dark mode

**Body:**

Every app that shows website icons hits the same problems: a blurry 16px `favicon.ico`, a black logo on a black dark-mode row, and Google's favicon endpoint quietly learning which sites your users have.

So I built **favicons.page**. One URL per site:

```
https://favicons.page/color/github.com
```

and you get the site's icon as a 128×128 SVG. SVG icons keep their exact shapes, and bitmaps are traced into vectors, so it scales cleanly to any size. A few things I'm happy with:

- **Light/dark colours in the response headers**: a badge fill, a readable text colour and a mask tint for both themes, checked against WCAG contrast.
- **Ready-made badges**: `/color/github.com/superellipse` is an app-style icon that switches between light and dark with the viewer's system theme. `/mono/` is a single-colour mask you can tint with `currentColor`.
- **Icon packs by name**: selfh.st, Simple Icons, Dashboard Icons and others, handy for self-hosted services that have no public favicon (`/color/selfhst/jellyfin`).
- **Safety**: every image is decoded in a throw-away seccomp sandbox and rebuilt by an allowlist SVG writer, so no scripts, no SSRF and no decompression bombs reach your page.
- **No key, no sign-up**, CORS is open, and there's an npm package with React/Vue components, a Svelte action and a web component.

It's written in Rust, MIT-licensed, and self-hostable as a ~9 MB Docker image if you'd rather run your own. The screenshots show it in Glance and Homepage dashboards.

- Live demo: https://favicons.page
- GitHub: https://github.com/peleda-app/favicons

I'd love feedback, especially icons that come out wrong or ugly. Those are the most useful bug reports.

---

# r/coolgithubprojects

**Type:** link post to the repository. Most posts there link to GitHub directly. Add the showcase image in a comment, or as the post image if the sub allows image posts.

**Link:** https://github.com/peleda-app/favicons

**Title:**

[Rust] favicons: any website's icon as a clean, sandboxed SVG with light/dark theme colours (self-hostable, no API key)

**First comment (post it yourself right after submitting):**

Hi, author here. favicons turns `https://favicons.page/color/{domain}` into the site's icon as a 128×128 SVG, plus light and dark theme colours in the response headers. It also has ready-made badges (`/superellipse` follows the system theme), single-colour masks and 17 icon packs by name.

Untrusted images are decoded in a seccomp sandbox and rebuilt by an allowlist SVG writer. It ships as a ~9 MB scratch Docker image, and there's a zero-dependency npm package with React/Vue/Svelte/web-component wrappers.

Demo: https://favicons.page. Happy to answer questions.

---

# Before posting

- Check each subreddit's current rules and pinned posts. I couldn't open Reddit from here.
- Post from your own account, and reply to comments for the first couple of hours. Engagement early on decides whether a post gets seen.
- Space the two posts a day or two apart, so they don't look like a campaign.
