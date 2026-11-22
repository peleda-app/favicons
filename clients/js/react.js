"use client";

import { createContext, createElement, useContext, useEffect, useMemo, useState } from "react";
import { clientFor, iconView } from "./shared.js";

const Settings = createContext({});

export function FaviconsProvider({ baseUrl, private: isPrivate, children }) {
  const value = useMemo(() => ({ baseUrl, private: isPrivate }), [baseUrl, isPrivate]);
  return createElement(Settings.Provider, { value }, children);
}

function useClient(baseUrl, isPrivate) {
  const inherited = useContext(Settings);
  return clientFor(baseUrl ?? inherited.baseUrl, isPrivate ?? inherited.private ?? false);
}

export function Favicon(props) {
  const { domain, catalog, slug, mono, shape, theme, size, alt, loading, baseUrl, private: isPrivate, ...rest } =
    props;
  const client = useClient(baseUrl, isPrivate);
  const view = iconView({ domain, catalog, slug, mono, shape, theme, size, alt, loading }, client);
  const style = view.style || rest.style ? { ...view.style, ...rest.style } : undefined;
  return createElement(view.tag, { ...view.attrs, ...rest, style });
}

export function useFavicon(target, options = {}) {
  const client = useClient(options.baseUrl, options.private);
  const key = JSON.stringify([client.baseUrl, client.private, target, options.style, options.shape, options.theme]);
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
  const client = useClient(options.baseUrl, options.private);
  return useMemo(
    () => client.url(target, options),
    [client, JSON.stringify(target), options.style, options.shape, options.theme],
  );
}
