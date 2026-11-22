import type { Shape, Theme } from "./index.js";

export interface FaviconParams {
  domain?: string;
  catalog?: string;
  slug?: string;
  mono?: boolean;
  shape?: Shape;
  theme?: Theme;
  size?: number | string;
  alt?: string;
  loading?: "lazy" | "eager";
  baseUrl?: string;
  private?: boolean;
}

export declare function favicon(
  node: HTMLElement,
  params: FaviconParams,
): { update(params: FaviconParams): void };
