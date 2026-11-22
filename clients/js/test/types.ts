import favicons, { createFavicons, iconUrl, type Icon, type Palette } from "favicons.page";

const own = createFavicons({ baseUrl: "https://icons.example.com", retries: 0 });
const a: string = iconUrl("github.com", { style: "mono", shape: "superellipse", theme: "dark" });
const b: string = own.url({ catalog: "selfhst", slug: "jellyfin" });
const c: string = favicons.mask("github.com", { shape: "rounded" }, { size: "80%" });

async function use(): Promise<string | undefined> {
  const icon: Icon = await own.get("github.com");
  const p: Palette | undefined = icon.palette;
  const reason = icon.fallback?.reason;
  if (reason === "queued") return undefined;
  const items = (await own.catalog("selfhst")).items.map((i) => i.slug);
  return `${a}${b}${c}${icon.svg}${p?.dark.plate}${items[0]}`;
}

void use;

import { createElement } from "react";
import { Favicon as ReactFavicon, FaviconsProvider, useFavicon } from "favicons.page/react";
import { Favicon as VueFavicon, FaviconsPlugin, useFaviconUrl } from "favicons.page/vue";
import { favicon } from "favicons.page/svelte";
import { defineFaviconElement } from "favicons.page/element";

const tree = createElement(
  FaviconsProvider,
  { baseUrl: "https://icons.example.com" },
  createElement(ReactFavicon, { domain: "github.com", shape: "superellipse", size: 40, className: "x" }),
);
function Hooked(): null {
  const { icon, loading } = useFavicon("github.com", { retries: 1 });
  void icon?.palette?.light.plate;
  void loading;
  return null;
}
const vueUrl: string = useFaviconUrl(() => "github.com", { style: "mono" }).value;
declare const node: HTMLElement;
favicon(node, { domain: "github.com", mono: true }).update({ catalog: "selfhst", slug: "jellyfin" });
const ctor: CustomElementConstructor | undefined = defineFaviconElement("site-icon", { baseUrl: "https://icons.example.com" });
void [tree, Hooked, VueFavicon, FaviconsPlugin, vueUrl, ctor];
