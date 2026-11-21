ARG BASE=ghcr.io/peleda-app/favicons.page:bare
FROM ${BASE}
ARG LICENSES="MIT"
COPY bundle /catalogs
LABEL org.opencontainers.image.title="favicons.page" \
      org.opencontainers.image.description="The favicons.page site: favicons with the page and prerendered icon catalogs; each catalog keeps its own license, see /catalogs/NOTICE" \
      org.opencontainers.image.licenses="${LICENSES}" \
      org.opencontainers.image.source="https://github.com/peleda-app/favicons"
