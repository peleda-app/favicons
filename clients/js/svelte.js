import { iconView, kebab } from "./shared.js";

const owned = new WeakMap();

function styleFor(view) {
  if (view.tag === "span") return view.style;
  return {
    display: "inline-block",
    width: view.attrs.width ? `${view.attrs.width}px` : view.style?.width,
    height: view.attrs.height ? `${view.attrs.height}px` : view.style?.height,
    backgroundImage: `url("${view.url}")`,
    backgroundSize: "contain",
    backgroundRepeat: "no-repeat",
    backgroundPosition: "center",
  };
}

function apply(node, params) {
  const view = iconView(params);
  if (node.tagName === "IMG") {
    node.setAttribute("src", view.url);
    for (const name of ["alt", "width", "height", "loading", "decoding"]) {
      const value = view.attrs[name];
      if (value !== undefined && !node.hasAttribute(name)) node.setAttribute(name, String(value));
    }
    node.setAttribute("referrerpolicy", "no-referrer");
    return;
  }
  const next = new Map(
    Object.entries(styleFor(view))
      .filter(([, v]) => v !== undefined)
      .map(([k, v]) => [kebab(k), String(v)]),
  );
  for (const name of owned.get(node) ?? []) {
    if (!next.has(name)) node.style.removeProperty(name);
  }
  for (const [name, value] of next) node.style.setProperty(name, value);
  owned.set(node, [...next.keys()]);
  if (!node.hasAttribute("role")) node.setAttribute("role", "img");
  if (params.alt) node.setAttribute("aria-label", params.alt);
  else if (!node.hasAttribute("aria-label")) node.setAttribute("aria-hidden", "true");
}

export function favicon(node, params) {
  apply(node, params);
  return {
    update(next) {
      apply(node, next);
    },
  };
}
