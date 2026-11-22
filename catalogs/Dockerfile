ARG BASE=ghcr.io/peleda-app/favicons.page:bare
FROM ${BASE}
ARG LICENSES="MIT"
COPY bundle /catalogs
COPY --from=sites /sites/ /catalogs/
LABEL org.opencontainers.image.title="favicons.page" \
      org.opencontainers.image.description="The favicons.page site: favicons with the page and prerendered icon catalogs and the icons of the 100,000 most popular sites; each catalog keeps its own license, see /catalogs/NOTICE and /catalogs/SOURCES.md" \
      org.opencontainers.image.licenses="${LICENSES}" \
      org.opencontainers.image.source="https://github.com/peleda-app/favicons"
