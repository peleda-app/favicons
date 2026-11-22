import { clientFor, cssText, iconView } from "./shared.js";

const ATTRIBUTES = ["domain", "catalog", "slug", "mono", "shape", "theme", "size", "alt", "base-url"];

function propsOf(el, baseUrl) {
  const get = (name) => el.getAttribute(name) ?? undefined;
  const size = get("size");
  return {
    domain: get("domain"),
    catalog: get("catalog"),
    slug: get("slug"),
    mono: el.hasAttribute("mono"),
    shape: get("shape"),
    theme: get("theme"),
    size: size === undefined ? undefined : /^\d+$/.test(size) ? Number(size) : size,
    alt: get("alt"),
    baseUrl: get("base-url") ?? baseUrl,
  };
}

export function defineFaviconElement(name = "favicon-icon", config = {}) {
  const registry = globalThis.customElements;
  if (!registry || typeof HTMLElement === "undefined") return undefined;
  const existing = registry.get(name);
  if (existing) return existing;

  class FaviconElement extends HTMLElement {
    static get observedAttributes() {
      return ATTRIBUTES;
    }

    connectedCallback() {
      this.render();
    }

    attributeChangedCallback() {
      if (this.isConnected) this.render();
    }

    render() {
      const props = propsOf(this, config.baseUrl);
      const root = this.shadowRoot ?? this.attachShadow({ mode: "open" });
      if (!props.domain && !props.slug) {
        root.replaceChildren();
        return;
      }
      let view;
      try {
        view = iconView(props, clientFor(props.baseUrl));
      } catch {
        root.replaceChildren();
        return;
      }
      const host = document.createElement("style");
      host.textContent = ":host{display:inline-block;line-height:0;vertical-align:middle}";
      const node = document.createElement(view.tag);
      for (const [attr, value] of Object.entries(view.attrs)) {
        if (value === undefined) continue;
        node.setAttribute(attr === "referrerPolicy" ? "referrerpolicy" : attr, String(value));
      }
      const css = cssText(view.style);
      if (css) node.setAttribute("style", css);
      node.setAttribute("part", "icon");
      root.replaceChildren(host, node);
    }
  }

  registry.define(name, FaviconElement);
  return FaviconElement;
}
