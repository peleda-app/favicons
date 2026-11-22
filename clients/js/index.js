export const DEFAULT_BASE_URL = "https://favicons.page";
export const PRIVATE_BASE_URL = "https://private.favicons.page";

const STYLES = new Set(["color", "mono"]);
const SHAPES = new Set(["rounded", "superellipse"]);
const THEMES = new Set(["light", "dark"]);
const TRANSIENT = new Set(["queued", "overloaded"]);
const SLUG = /^[a-z0-9][a-z0-9._-]*$/;

export class FaviconsError extends Error {
  constructor(message, { status, url, response } = {}) {
    super(message);
    this.name = "FaviconsError";
    this.status = status;
    this.url = url;
    this.response = response;
  }
}

export function normalizeDomain(input) {
  let value = String(input ?? "").trim();
  if (!value) throw new TypeError("favicons: domain is empty");
  if (!/^[a-z][a-z0-9+.-]*:\/\//i.test(value)) value = `http://${value}`;
  let host;
  try {
    host = new URL(value).hostname;
  } catch {
    throw new TypeError(`favicons: not a domain: ${input}`);
  }
  host = host.replace(/\.+$/, "").replace(/^www\./, "");
  if (!host || host.startsWith("[")) throw new TypeError(`favicons: not a domain: ${input}`);
  return host;
}

function target(input) {
  if (typeof input === "object" && input !== null && !(input instanceof URL)) {
    const catalog = String(input.catalog ?? "").toLowerCase();
    const slug = String(input.slug ?? "").toLowerCase();
    if (!SLUG.test(catalog) || !SLUG.test(slug)) {
      throw new TypeError("favicons: catalog and slug must be lower-case names");
    }
    return `${catalog}/${slug}`;
  }
  return normalizeDomain(input);
}

function path(input, options = {}) {
  const style = options.style ?? "color";
  if (!STYLES.has(style)) throw new TypeError(`favicons: unknown style ${style}`);
  const parts = [style, target(input)];
  if (options.shape !== undefined) {
    if (!SHAPES.has(options.shape)) throw new TypeError(`favicons: unknown shape ${options.shape}`);
    parts.push(options.shape);
  }
  if (options.theme !== undefined && options.theme !== "auto") {
    if (!THEMES.has(options.theme)) throw new TypeError(`favicons: unknown theme ${options.theme}`);
    if (options.shape === undefined) throw new TypeError("favicons: a theme needs a shape");
    parts.push(options.theme);
  }
  return `/${parts.join("/")}`;
}

function scheme(headers, theme) {
  const get = (name) => headers.get(`x-icon-${theme}-${name}`) ?? undefined;
  const plate = get("plate");
  if (plate === undefined) return undefined;
  return { plate, text: get("text"), mono: get("mono"), monoPlate: get("mono-plate") };
}

function palette(headers) {
  const light = scheme(headers, "light");
  const dark = scheme(headers, "dark");
  if (!light || !dark) return undefined;
  return {
    shape: headers.get("x-icon-shape") ?? undefined,
    fill: headers.get("x-icon-fill") ?? undefined,
    accent: headers.get("x-icon-accent") ?? undefined,
    light,
    dark,
  };
}

function describe(response) {
  const h = response.headers;
  const opt = (name) => h.get(name) ?? undefined;
  const reason = opt("x-icon-fallback-reason");
  return {
    url: response.url,
    status: response.status,
    source: opt("x-icon-source"),
    domain: opt("x-icon-domain"),
    style: opt("x-icon-style"),
    vector: opt("x-icon-vector"),
    monoSource: opt("x-icon-mono-source"),
    provenance: opt("x-icon-provenance"),
    license: opt("x-icon-license"),
    upstream: opt("x-icon-upstream"),
    fallback: h.get("x-icon-fallback") === "true" ? { reason } : undefined,
    blocked: opt("x-icon-blocked"),
    stale: h.get("x-icon-stale") === "true",
    etag: opt("etag"),
    palette: palette(h),
    response,
  };
}

function wait(ms, signal) {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(signal.reason);
    const timer = setTimeout(done, ms);
    function done() {
      signal?.removeEventListener("abort", abort);
      resolve();
    }
    function abort() {
      clearTimeout(timer);
      reject(signal.reason);
    }
    signal?.addEventListener("abort", abort, { once: true });
  });
}

export function maskCss(url, { size = "contain", position = "center" } = {}) {
  const value = `url("${String(url).replace(/["\\\n]/g, encodeURIComponent)}") ${position} / ${size} no-repeat`;
  return `-webkit-mask: ${value}; mask: ${value};`;
}

function trimmed(value) {
  return String(value).replace(/\/+$/, "");
}

export function createFavicons(config = {}) {
  const isPrivate = Boolean(config.private);
  const shared = trimmed(config.baseUrl ?? DEFAULT_BASE_URL);
  const privateBase =
    config.privateBaseUrl !== undefined
      ? trimmed(config.privateBaseUrl)
      : shared === DEFAULT_BASE_URL
        ? PRIVATE_BASE_URL
        : undefined;
  const base = isPrivate && privateBase ? privateBase : shared;
  const asked = isPrivate && !privateBase ? { "x-icon-private": "1" } : undefined;
  const fetcher = config.fetch ?? globalThis.fetch;
  const retries = config.retries ?? 2;
  const retryDelay = config.retryDelay ?? 1500;

  function url(input, options) {
    return `${base}${path(input, options)}`;
  }

  async function send(address, init, reload) {
    if (typeof fetcher !== "function") {
      throw new FaviconsError("favicons: no fetch available, pass one in createFavicons({ fetch })");
    }
    if (asked) init = { ...init, headers: { ...init.headers, ...asked } };
    if (!reload) return fetcher(address, init);
    try {
      return await fetcher(address, { ...init, cache: "reload" });
    } catch (error) {
      if (error instanceof TypeError && /cache/i.test(error.message)) return fetcher(address, init);
      throw error;
    }
  }

  async function request(input, options = {}, method = "GET") {
    const address = url(input, options);
    const attempts = 1 + Math.max(0, options.retries ?? retries);
    const delay = options.retryDelay ?? retryDelay;
    for (let attempt = 1; ; attempt++) {
      const response = await send(address, { method, signal: options.signal }, attempt > 1);
      const last = attempt >= attempts;
      if (response.status === 429 || response.status === 503) {
        if (last) {
          throw new FaviconsError(`favicons: ${response.status} for ${address}`, {
            status: response.status,
            url: address,
            response,
          });
        }
        const after = Number(response.headers.get("retry-after"));
        await wait(Number.isFinite(after) && after > 0 ? after * 1000 : delay, options.signal);
        continue;
      }
      if (!response.ok) {
        throw new FaviconsError(`favicons: ${response.status} for ${address}`, {
          status: response.status,
          url: address,
          response,
        });
      }
      const info = describe(response);
      if (!last && TRANSIENT.has(info.fallback?.reason)) {
        await response.body?.cancel?.();
        await wait(delay, options.signal);
        continue;
      }
      return info;
    }
  }

  async function get(input, options) {
    const info = await request(input, options, "GET");
    return { ...info, svg: await info.response.text() };
  }

  async function head(input, options) {
    return request(input, options, "HEAD");
  }

  async function json(route, signal) {
    const address = `${base}${route}`;
    const response = await send(address, { signal }, false);
    if (!response.ok) {
      throw new FaviconsError(`favicons: ${response.status} for ${address}`, {
        status: response.status,
        url: address,
        response,
      });
    }
    return response.json();
  }

  return {
    baseUrl: base,
    private: isPrivate,
    url,
    get,
    head,
    mask: (input, options = {}, css) => maskCss(url(input, { ...options, style: "mono" }), css),
    catalogs: (options = {}) => json("/catalogs.json", options.signal),
    catalog: (id, options = {}) => {
      if (!SLUG.test(String(id))) throw new TypeError("favicons: catalog must be a lower-case name");
      return json(`/catalogs/${id}.json`, options.signal);
    },
  };
}

const defaults = createFavicons();

export const iconUrl = defaults.url;
export const getIcon = defaults.get;
export const headIcon = defaults.head;
export default defaults;
