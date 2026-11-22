"use client";

import { createContext, createElement, useContext, useEffect, useMemo, useState } from "react";
import { clientFor, iconView } from "./shared.js";

const BaseUrl = createContext(undefined);

export function FaviconsProvider({ baseUrl, children }) {
  return createElement(BaseUrl.Provider, { value: baseUrl }, children);
}

function useClient(baseUrl) {
  const inherited = useContext(BaseUrl);
  return clientFor(baseUrl ?? inherited);
}

export function Favicon(props) {
  const { domain, catalog, slug, mono, shape, theme, size, alt, loading, baseUrl, ...rest } = props;
  const client = useClient(baseUrl);
  const view = iconView({ domain, catalog, slug, mono, shape, theme, size, alt, loading }, client);
  const style = view.style || rest.style ? { ...view.style, ...rest.style } : undefined;
  return createElement(view.tag, { ...view.attrs, ...rest, style });
}

export function useFavicon(target, options = {}) {
  const client = useClient(options.baseUrl);
  const key = JSON.stringify([client.baseUrl, target, options.style, options.shape, options.theme]);
  const [state, setState] = useState({ icon: undefined, error: undefined, loading: true });
  useEffect(() => {
    const controller = new AbortController();
    setState((s) => ({ icon: s.icon, error: undefined, loading: true }));
    client
      .get(target, { ...options, signal: controller.signal })
      .then((icon) => setState({ icon, error: undefined, loading: false }))
      .catch((error) => {
        if (!controller.signal.aborted) setState({ icon: undefined, error, loading: false });
      });
    return () => controller.abort();
  }, [key]);
  return state;
}

export function useFaviconUrl(target, options = {}) {
  const client = useClient(options.baseUrl);
  return useMemo(
    () => client.url(target, options),
    [client, JSON.stringify(target), options.style, options.shape, options.theme],
  );
}
