import assert from "node:assert/strict";
import { test } from "node:test";
import favicons, {
  createFavicons,
  DEFAULT_BASE_URL,
  FaviconsError,
  iconUrl,
  maskCss,
  normalizeDomain,
} from "../index.js";

const SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"/>';

function reply(status, headers = {}, body = SVG) {
  return new Response(status === 304 ? null : body, { status, headers });
}

function scripted(...responses) {
  const calls = [];
  const fetch = async (url, init) => {
    calls.push({ url, init });
    const next = responses.shift();
    if (!next) throw new Error("unexpected request");
    return typeof next === "function" ? next(url, init) : next;
  };
  return { fetch, calls };
}

test("builds canonical urls on favicons.page by default", () => {
  assert.equal(DEFAULT_BASE_URL, "https://favicons.page");
  assert.equal(favicons.baseUrl, DEFAULT_BASE_URL);
  assert.equal(iconUrl("github.com"), "https://favicons.page/color/github.com");
  assert.equal(
    iconUrl("https://www.GitHub.com/features?x=1", { style: "mono" }),
    "https://favicons.page/mono/github.com",
  );
  assert.equal(
    iconUrl("github.com", { shape: "superellipse", theme: "dark" }),
    "https://favicons.page/color/github.com/superellipse/dark",
  );
  assert.equal(
    iconUrl("github.com", { shape: "rounded", theme: "auto" }),
    "https://favicons.page/color/github.com/rounded",
  );
  assert.equal(
    iconUrl({ catalog: "selfhst", slug: "Jellyfin" }, { style: "mono", shape: "rounded" }),
    "https://favicons.page/mono/selfhst/jellyfin/rounded",
  );
});

test("a custom base url replaces favicons.page", () => {
  const own = createFavicons({ baseUrl: "https://icons.example.com/favicons/" });
  assert.equal(own.baseUrl, "https://icons.example.com/favicons");
  assert.equal(own.url("example.org"), "https://icons.example.com/favicons/color/example.org");
});

test("domains are normalized and international names become punycode", () => {
  assert.equal(normalizeDomain(" Example.COM. "), "example.com");
  assert.equal(normalizeDomain("example.com:8443/path"), "example.com");
  assert.equal(normalizeDomain(new URL("https://www.example.co.uk/a")), "example.co.uk");
  assert.equal(normalizeDomain("пример.рф"), "xn--e1afmkfd.xn--p1ai");
  assert.equal(normalizeDomain("🍕.ws"), "xn--vi8h.ws");
  assert.throws(() => normalizeDomain(""), TypeError);
  assert.throws(() => normalizeDomain("http://[::1]/"), TypeError);
});

test("bad options fail before any request", () => {
  assert.throws(() => iconUrl("github.com", { style: "grey" }), TypeError);
  assert.throws(() => iconUrl("github.com", { shape: "circle" }), TypeError);
  assert.throws(() => iconUrl("github.com", { theme: "dark" }), /needs a shape/);
  assert.throws(() => iconUrl({ catalog: "../x", slug: "a" }), TypeError);
});

test("get returns the svg with its palette and provenance", async () => {
  const { fetch, calls } = scripted(
    reply(200, {
      "x-icon-source": "cache",
      "x-icon-domain": "discord.com",
      "x-icon-style": "color",
      "x-icon-vector": "original",
      "x-icon-provenance": "site",
      etag: '"abc"',
      "x-icon-shape": "square",
      "x-icon-fill": "bleed",
      "x-icon-accent": "#5a67f2",
      "x-icon-light-plate": "#ebecfa",
      "x-icon-light-text": "#4752c4",
      "x-icon-light-mono": "#5a67f2",
      "x-icon-light-mono-plate": "#eef0fd",
      "x-icon-dark-plate": "#181b3f",
      "x-icon-dark-text": "#6b77f3",
      "x-icon-dark-mono": "#6b77f3",
      "x-icon-dark-mono-plate": "#1d2147",
    }),
  );
  const icon = await createFavicons({ fetch }).get("discord.com");
  assert.equal(calls[0].url, "https://favicons.page/color/discord.com");
  assert.equal(calls[0].init.method, "GET");
  assert.equal(icon.svg, SVG);
  assert.equal(icon.source, "cache");
  assert.equal(icon.vector, "original");
  assert.equal(icon.fallback, undefined);
  assert.equal(icon.stale, false);
  assert.deepEqual(icon.palette, {
    shape: "square",
    fill: "bleed",
    accent: "#5a67f2",
    light: { plate: "#ebecfa", text: "#4752c4", mono: "#5a67f2", monoPlate: "#eef0fd" },
    dark: { plate: "#181b3f", text: "#6b77f3", mono: "#6b77f3", monoPlate: "#1d2147" },
  });
});

test("a queued icon is fetched again until it is ready", async () => {
  const { fetch, calls } = scripted(
    reply(200, { "x-icon-source": "generated", "x-icon-fallback": "true", "x-icon-fallback-reason": "queued" }),
    reply(200, { "x-icon-source": "origin" }),
  );
  const icon = await createFavicons({ fetch, retryDelay: 1 }).get("example.org");
  assert.equal(icon.source, "origin");
  assert.equal(calls.length, 2);
  assert.equal(calls[0].init.cache, undefined);
  assert.equal(calls[1].init.cache, "reload");
});

test("retries stop at the limit and return the fallback", async () => {
  const queued = () =>
    reply(200, { "x-icon-source": "generated", "x-icon-fallback": "true", "x-icon-fallback-reason": "queued" });
  const { fetch, calls } = scripted(queued(), queued());
  const icon = await createFavicons({ fetch, retries: 1, retryDelay: 1 }).get("example.org");
  assert.equal(calls.length, 2);
  assert.deepEqual(icon.fallback, { reason: "queued" });
});

test("stable fallbacks are returned at once", async () => {
  const { fetch, calls } = scripted(
    reply(200, {
      "x-icon-source": "generated",
      "x-icon-fallback": "true",
      "x-icon-fallback-reason": "blocklisted",
      "x-icon-blocked": "blocklist",
    }),
  );
  const icon = await createFavicons({ fetch }).get("bad.example");
  assert.equal(calls.length, 1);
  assert.equal(icon.blocked, "blocklist");
  assert.deepEqual(icon.fallback, { reason: "blocklisted" });
});

test("rate limits honour retry-after and then give up with an error", async () => {
  const { fetch, calls } = scripted(
    reply(429, { "retry-after": "0" }, "slow down"),
    reply(503, {}, "overloaded"),
  );
  await assert.rejects(
    createFavicons({ fetch, retries: 1, retryDelay: 1 }).get("github.com"),
    (error) => error instanceof FaviconsError && error.status === 503,
  );
  assert.equal(calls.length, 2);
});

test("a 404 throws without retrying", async () => {
  const { fetch, calls } = scripted(reply(404, {}, "not found"));
  await assert.rejects(
    createFavicons({ fetch }).get({ catalog: "selfhst", slug: "nope" }),
    (error) => error instanceof FaviconsError && error.status === 404,
  );
  assert.equal(calls.length, 1);
});

test("head reads headers without a body", async () => {
  const { fetch, calls } = scripted(reply(200, { "x-icon-source": "cache", "x-icon-stale": "true" }, null));
  const info = await createFavicons({ fetch }).head("github.com", { style: "mono" });
  assert.equal(calls[0].init.method, "HEAD");
  assert.equal(calls[0].url, "https://favicons.page/mono/github.com");
  assert.equal(info.stale, true);
  assert.equal("svg" in info, false);
});

test("a fetch without cache modes still gets its retry", async () => {
  const { fetch, calls } = scripted(
    reply(200, { "x-icon-fallback": "true", "x-icon-fallback-reason": "overloaded" }),
    (_, init) => {
      if (init.cache) throw new TypeError("cache mode is not implemented");
      return reply(200, { "x-icon-source": "origin" });
    },
    reply(200, { "x-icon-source": "origin" }),
  );
  const icon = await createFavicons({ fetch, retryDelay: 1 }).get("example.org");
  assert.equal(icon.source, "origin");
  assert.equal(calls.length, 3);
});

test("an aborted request stops waiting", async () => {
  const controller = new AbortController();
  const { fetch } = scripted(
    reply(200, { "x-icon-fallback": "true", "x-icon-fallback-reason": "queued" }),
  );
  const pending = createFavicons({ fetch, retryDelay: 10_000 }).get("example.org", {
    signal: controller.signal,
  });
  setTimeout(() => controller.abort(new Error("stop")), 5);
  await assert.rejects(pending, /stop/);
});

test("catalog listings come back as json", async () => {
  const { fetch, calls } = scripted(
    reply(200, { "content-type": "application/json" }, JSON.stringify([{ id: "selfhst", count: 1 }])),
    reply(200, { "content-type": "application/json" }, JSON.stringify({ id: "selfhst", items: [] })),
  );
  const own = createFavicons({ fetch });
  assert.deepEqual(await own.catalogs(), [{ id: "selfhst", count: 1 }]);
  assert.deepEqual(await own.catalog("selfhst"), { id: "selfhst", items: [] });
  assert.equal(calls[1].url, "https://favicons.page/catalogs/selfhst.json");
  assert.throws(() => own.catalog("../etc"), TypeError);
});

test("mask helpers produce css for the mono icon", () => {
  assert.equal(
    favicons.mask("github.com"),
    '-webkit-mask: url("https://favicons.page/mono/github.com") center / contain no-repeat; mask: url("https://favicons.page/mono/github.com") center / contain no-repeat;',
  );
  assert.match(maskCss('https://x.test/a"b', { size: "80%" }), /url\("https:\/\/x\.test\/a%22b"\) center \/ 80% no-repeat/);
});

test("the live service answers through the client", { skip: !process.env.FAVICONS_LIVE }, async () => {
  const icon = await favicons.get("github.com", { shape: "superellipse" });
  assert.match(icon.svg, /^<svg/);
  assert.ok(icon.palette?.light.plate);
});
