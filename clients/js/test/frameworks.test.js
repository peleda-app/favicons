import assert from "node:assert/strict";
import { test } from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createSSRApp, h } from "vue";
import { renderToString } from "vue/server-renderer";
import { defineFaviconElement } from "../element.js";
import * as react from "../react.js";
import { cssText, iconView } from "../shared.js";
import { favicon } from "../svelte.js";
import * as vue from "../vue.js";

test("a colour icon is an img and a bare mono icon is a tintable mask", () => {
  const img = iconView({ domain: "github.com", shape: "superellipse", size: 40, alt: "GitHub" });
  assert.equal(img.tag, "img");
  assert.equal(img.attrs.src, "https://favicons.page/color/github.com/superellipse");
  assert.equal(img.attrs.width, 40);
  assert.equal(img.attrs.alt, "GitHub");

  const mask = iconView({ domain: "github.com", mono: true, size: "1em" });
  assert.equal(mask.tag, "span");
  assert.equal(mask.attrs["aria-hidden"], "true");
  assert.equal(
    cssText(mask.style),
    'display:inline-block;width:1em;height:1em;background-color:currentColor;-webkit-mask:url("https://favicons.page/mono/github.com") center / contain no-repeat;mask:url("https://favicons.page/mono/github.com") center / contain no-repeat',
  );

  const badge = iconView({ catalog: "selfhst", slug: "jellyfin", mono: true, shape: "rounded", theme: "dark" });
  assert.equal(badge.tag, "img");
  assert.equal(badge.url, "https://favicons.page/mono/selfhst/jellyfin/rounded/dark");
});

test("private mode reaches every framework entry", async () => {
  assert.equal(iconView({ domain: "github.com", private: true }).url, "https://private.favicons.page/color/github.com");
  const html = renderToStaticMarkup(
    createElement(
      react.FaviconsProvider,
      { private: true },
      createElement(react.Favicon, { domain: "github.com" }),
      createElement(react.Favicon, { domain: "gitlab.com", private: false }),
    ),
  );
  assert.match(html, /https:\/\/private\.favicons\.page\/color\/github\.com/);
  assert.match(html, /"https:\/\/favicons\.page\/color\/gitlab\.com"/);
  const app = createSSRApp({ render: () => h(vue.Favicon, { domain: "github.com" }) });
  app.use(vue.FaviconsPlugin, { private: true });
  assert.match(await renderToString(app), /private\.favicons\.page\/color\/github\.com/);
});

test("react renders the icon and takes the base url from the provider", () => {
  const html = renderToStaticMarkup(
    createElement(
      react.FaviconsProvider,
      { baseUrl: "https://icons.example.com" },
      createElement(react.Favicon, { domain: "github.com", shape: "rounded", className: "icon" }),
      createElement(react.Favicon, { domain: "github.com", mono: true, size: 16, alt: "GitHub" }),
    ),
  );
  assert.match(html, /<img src="https:\/\/icons\.example\.com\/color\/github\.com\/rounded"/);
  assert.match(html, /class="icon"/);
  assert.match(html, /loading="lazy"/);
  assert.match(html, /referrerPolicy="no-referrer"|referrerpolicy="no-referrer"/);
  assert.match(html, /<span role="img" aria-label="GitHub" style="display:inline-block;width:16px;height:16px;background-color:currentColor;-webkit-mask:url\(&quot;https:\/\/icons\.example\.com\/mono\/github\.com&quot;\)/);
});

test("react hooks build urls during render", () => {
  let url;
  function Probe() {
    url = react.useFaviconUrl("github.com", { style: "mono" });
    return null;
  }
  renderToStaticMarkup(createElement(Probe));
  assert.equal(url, "https://favicons.page/mono/github.com");
});

test("vue renders the icon and takes the base url from the plugin", async () => {
  const app = createSSRApp({
    render: () => [
      h(vue.Favicon, { domain: "github.com", shape: "superellipse", theme: "light", class: "icon" }),
      h(vue.Favicon, { catalog: "simple-icons", slug: "github", mono: true }),
    ],
  });
  app.use(vue.FaviconsPlugin, { baseUrl: "https://icons.example.com" });
  const html = await renderToString(app);
  assert.match(html, /<img src="https:\/\/icons\.example\.com\/color\/github\.com\/superellipse\/light"/);
  assert.match(html, /class="icon"/);
  assert.match(html, /referrerpolicy="no-referrer"/);
  assert.match(html, /<span aria-hidden="true" style="display:inline-block;width:32px;height:32px;background-color:currentColor;-webkit-mask:url\(&quot;https:\/\/icons\.example\.com\/mono\/simple-icons\/github&quot;\)/);
});

test("vue composables return reactive urls", async () => {
  let url;
  const app = createSSRApp({
    setup() {
      url = vue.useFaviconUrl(() => "github.com", { shape: "rounded" });
      return () => null;
    },
  });
  await renderToString(app);
  assert.equal(url.value, "https://favicons.page/color/github.com/rounded");
});

function fakeNode(tagName) {
  const attrs = new Map();
  const style = new Map();
  return {
    tagName,
    attrs,
    css: style,
    setAttribute: (k, v) => attrs.set(k, v),
    getAttribute: (k) => attrs.get(k) ?? null,
    hasAttribute: (k) => attrs.has(k),
    style: {
      setProperty: (k, v) => style.set(k, v),
      removeProperty: (k) => style.delete(k),
    },
  };
}

test("the svelte action fills an img and turns other elements into a mask", () => {
  const img = fakeNode("IMG");
  img.setAttribute("alt", "mine");
  const action = favicon(img, { domain: "github.com", size: 24 });
  assert.equal(img.attrs.get("src"), "https://favicons.page/color/github.com");
  assert.equal(img.attrs.get("alt"), "mine");
  assert.equal(img.attrs.get("width"), "24");
  action.update({ domain: "gitlab.com", size: 24 });
  assert.equal(img.attrs.get("src"), "https://favicons.page/color/gitlab.com");

  const span = fakeNode("SPAN");
  const mask = favicon(span, { domain: "github.com", mono: true });
  assert.equal(span.css.get("background-color"), "currentColor");
  assert.match(span.css.get("mask"), /mono\/github\.com/);
  assert.equal(span.attrs.get("aria-hidden"), "true");
  mask.update({ domain: "github.com", shape: "rounded" });
  assert.equal(span.css.has("mask"), false);
  assert.match(span.css.get("background-image"), /color\/github\.com\/rounded/);
});

test("the web component is a no-op where custom elements do not exist", () => {
  assert.equal(defineFaviconElement(), undefined);
});

test("the react entry is a client module for server components", async () => {
  const { readFile } = await import("node:fs/promises");
  const source = await readFile(new URL("../react.js", import.meta.url), "utf8");
  assert.match(source, /^"use client";\n/);
});

test("hooks render a loading state on the server without fetching", async () => {
  const real = globalThis.fetch;
  let calls = 0;
  globalThis.fetch = async () => {
    calls++;
    throw new Error("no fetch during ssr");
  };
  try {
    let reactState;
    function Probe() {
      reactState = react.useFavicon("github.com", { baseUrl: "https://ssr-react.example.com" });
      return null;
    }
    renderToStaticMarkup(createElement(Probe));

    let vueState;
    const app = createSSRApp({
      setup() {
        vueState = vue.useFavicon("github.com", { baseUrl: "https://ssr-vue.example.com" });
        return () => null;
      },
    });
    await renderToString(app);

    assert.equal(calls, 0);
    assert.equal(reactState.loading, true);
    assert.equal(reactState.icon, undefined);
    assert.equal(vueState.loading.value, true);
    assert.equal(vueState.icon.value, undefined);
  } finally {
    globalThis.fetch = real;
  }
});
