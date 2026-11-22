import type { DefineComponent, MaybeRefOrGetter, Plugin, Ref, ComputedRef } from "vue";
import type { Icon, IconTarget, RequestOptions, Shape, Theme, UrlOptions } from "./index.js";

export interface FaviconProps {
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

export declare const Favicon: DefineComponent<FaviconProps>;

export declare const FaviconsPlugin: Plugin<[{ baseUrl?: string; private?: boolean }?]>;

export declare function useFavicon(
  target: MaybeRefOrGetter<IconTarget>,
  options?: MaybeRefOrGetter<Omit<RequestOptions, "signal"> & { baseUrl?: string; private?: boolean }>,
): { icon: Ref<Icon | undefined>; error: Ref<unknown>; loading: Ref<boolean> };

export declare function useFaviconUrl(
  target: MaybeRefOrGetter<IconTarget>,
  options?: MaybeRefOrGetter<UrlOptions & { baseUrl?: string; private?: boolean }>,
): ComputedRef<string>;
