import { computed, defineComponent, h, inject, onScopeDispose, ref, toValue, watch } from "vue";
import { clientFor, cssText, iconView } from "./shared.js";

const SETTINGS = Symbol("favicons.settings");

export const FaviconsPlugin = {
  install(app, options = {}) {
    app.provide(SETTINGS, { baseUrl: options.baseUrl, private: options.private });
  },
};

function useClient(baseUrl, isPrivate) {
  const inherited = inject(SETTINGS, {});
  return clientFor(baseUrl ?? inherited.baseUrl, isPrivate ?? inherited.private ?? false);
}

export const Favicon = defineComponent({
  name: "Favicon",
  inheritAttrs: true,
  props: {
    domain: String,
    catalog: String,
    slug: String,
    mono: Boolean,
    shape: String,
    theme: String,
    size: { type: [Number, String], default: 32 },
    alt: { type: String, default: "" },
    loading: String,
    baseUrl: String,
    private: { type: Boolean, default: undefined },
  },
  setup(props) {
    const inherited = inject(SETTINGS, {});
    return () => {
      const client = clientFor(
        props.baseUrl ?? inherited.baseUrl,
        props.private ?? inherited.private ?? false,
      );
      const view = iconView(props, client);
      const { referrerPolicy, ...attrs } = view.attrs;
      if (referrerPolicy) attrs.referrerpolicy = referrerPolicy;
      return h(view.tag, { ...attrs, style: cssText(view.style) || undefined });
    };
  },
});

export function useFavicon(target, options = {}) {
  const client = useClient(toValue(options).baseUrl, toValue(options).private);
  const icon = ref();
  const error = ref();
  const loading = ref(true);
  let controller;
  if (typeof window === "undefined") return { icon, error, loading };
  const stop = watch(
    () => [JSON.stringify(toValue(target)), JSON.stringify(toValue(options))],
    () => {
      controller?.abort();
      controller = new AbortController();
      const signal = controller.signal;
      loading.value = true;
      error.value = undefined;
      client
        .get(toValue(target), { ...toValue(options), signal })
        .then((value) => {
          icon.value = value;
          loading.value = false;
        })
        .catch((e) => {
          if (signal.aborted) return;
          icon.value = undefined;
          error.value = e;
          loading.value = false;
        });
    },
    { immediate: true },
  );
  onScopeDispose(() => {
    stop();
    controller?.abort();
  });
  return { icon, error, loading };
}

export function useFaviconUrl(target, options = {}) {
  const client = useClient(toValue(options).baseUrl, toValue(options).private);
  return computed(() => client.url(toValue(target), toValue(options)));
}
