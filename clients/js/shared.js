import { createFavicons, DEFAULT_BASE_URL } from "./index.js";

const clients = new Map();

export function clientFor(baseUrl, isPrivate = false) {
  const base = baseUrl ?? DEFAULT_BASE_URL;
  const key = `${isPrivate ? "private " : ""}${base}`;
  let client = clients.get(key);
  if (!client) {
    client = createFavicons({ baseUrl: base, private: isPrivate });
    clients.set(key, client);
  }
  return client;
}

export function targetOf(props) {
  if (props.catalog !== undefined || props.slug !== undefined) {
    return { catalog: props.catalog, slug: props.slug };
  }
  return props.domain;
}

export function iconView(props, client = clientFor(props.baseUrl, props.private)) {
  const size = props.size ?? 32;
  const px = typeof size === "number" ? `${size}px` : size;
  const alt = props.alt ?? "";
  const target = targetOf(props);
  if (props.mono && props.shape === undefined) {
    const url = client.url(target, { style: "mono" });
    const mask = `url("${url}") center / contain no-repeat`;
    return {
      tag: "span",
      url,
      attrs: alt ? { role: "img", "aria-label": alt } : { "aria-hidden": "true" },
      style: {
        display: "inline-block",
        width: px,
        height: px,
        backgroundColor: "currentColor",
        WebkitMask: mask,
        mask,
      },
    };
  }
  const url = client.url(target, {
    style: props.mono ? "mono" : "color",
    shape: props.shape,
    theme: props.theme,
  });
  return {
    tag: "img",
    url,
    attrs: {
      src: url,
      alt,
      width: typeof size === "number" ? size : undefined,
      height: typeof size === "number" ? size : undefined,
      loading: props.loading ?? "lazy",
      decoding: "async",
      referrerPolicy: "no-referrer",
    },
    style: typeof size === "number" ? undefined : { width: px, height: px },
  };
}

export function kebab(name) {
  return name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
}

export function cssText(style) {
  if (!style) return "";
  return Object.entries(style)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => `${kebab(k)}:${v}`)
    .join(";");
}
