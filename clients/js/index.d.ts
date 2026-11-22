export declare const DEFAULT_BASE_URL: "https://favicons.page";
export declare const PRIVATE_BASE_URL: "https://private.favicons.page";

export type Style = "color" | "mono";
export type Shape = "rounded" | "superellipse";
export type Theme = "light" | "dark" | "auto";

export type FallbackReason =
  | "queued"
  | "overloaded"
  | "fetch-failed"
  | "not-found"
  | "domain-too-new"
  | "blocklisted"
  | "dns-blocked"
  | "no-monochrome"
  | "invalid-domain"
  | "not-cached";

export interface CatalogIcon {
  catalog: string;
  slug: string;
}

export type IconTarget = string | URL | CatalogIcon;

export interface UrlOptions {
  style?: Style;
  shape?: Shape;
  theme?: Theme;
}

export interface RequestOptions extends UrlOptions {
  retries?: number;
  retryDelay?: number;
  signal?: AbortSignal;
}

export interface Scheme {
  plate: string;
  text?: string;
  mono?: string;
  monoPlate?: string;
}

export interface Palette {
  shape?: "square" | "rounded" | "circle" | "none";
  fill?: "bleed" | "inset";
  accent?: string;
  light: Scheme;
  dark: Scheme;
}

export interface IconInfo {
  url: string;
  status: number;
  source?: "cache" | "origin" | "generated";
  domain?: string;
  style?: Style;
  vector?: "original" | "traced" | "generated";
  monoSource?: "mask-icon" | "derived" | "generated";
  provenance?: "bimi" | "thesvg" | "site" | "upstream" | "catalog";
  license?: string;
  upstream?: string;
  fallback?: { reason?: FallbackReason };
  blocked?: "blocklist" | "dns-filter";
  stale: boolean;
  etag?: string;
  palette?: Palette;
  response: Response;
}

export interface Icon extends IconInfo {
  svg: string;
}

export interface MaskOptions {
  size?: string;
  position?: string;
}

export interface CatalogSummary {
  id: string;
  name: string;
  license?: string;
  attribution?: string;
  count: number;
}

export interface CatalogItem {
  slug: string;
  name: string;
  category?: string;
  tags?: string[];
}

export interface Catalog extends CatalogSummary {
  items: CatalogItem[];
}

export interface FaviconsConfig {
  baseUrl?: string;
  private?: boolean;
  privateBaseUrl?: string;
  fetch?: typeof fetch;
  retries?: number;
  retryDelay?: number;
}

export interface Favicons {
  readonly baseUrl: string;
  readonly private: boolean;
  url(target: IconTarget, options?: UrlOptions): string;
  get(target: IconTarget, options?: RequestOptions): Promise<Icon>;
  head(target: IconTarget, options?: RequestOptions): Promise<IconInfo>;
  mask(target: IconTarget, options?: Omit<UrlOptions, "style">, css?: MaskOptions): string;
  catalogs(options?: { signal?: AbortSignal }): Promise<CatalogSummary[]>;
  catalog(id: string, options?: { signal?: AbortSignal }): Promise<Catalog>;
}

export declare class FaviconsError extends Error {
  readonly status?: number;
  readonly url?: string;
  readonly response?: Response;
}

export declare function createFavicons(config?: FaviconsConfig): Favicons;
export declare function normalizeDomain(input: string | URL): string;
export declare function maskCss(url: string, options?: MaskOptions): string;

export declare const iconUrl: Favicons["url"];
export declare const getIcon: Favicons["get"];
export declare const headIcon: Favicons["head"];

declare const favicons: Favicons;
export default favicons;
