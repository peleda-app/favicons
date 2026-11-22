import type { HTMLAttributes, ReactElement, ReactNode } from "react";
import type { Icon, IconTarget, RequestOptions, Shape, Theme, UrlOptions } from "./index.js";

export interface FaviconProps extends Omit<HTMLAttributes<HTMLElement>, "children"> {
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

export declare function Favicon(props: FaviconProps): ReactElement;

export declare function FaviconsProvider(props: {
  baseUrl?: string;
  private?: boolean;
  children?: ReactNode;
}): ReactElement;

export interface FaviconState {
  icon: Icon | undefined;
  error: unknown;
  loading: boolean;
}

export declare function useFavicon(
  target: IconTarget,
  options?: Omit<RequestOptions, "signal"> & { baseUrl?: string; private?: boolean },
): FaviconState;

export declare function useFaviconUrl(
  target: IconTarget,
  options?: UrlOptions & { baseUrl?: string; private?: boolean },
): string;
