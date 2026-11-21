ARG BASE=ghcr.io/peleda-app/favicons-site:latest
FROM ${BASE}
ARG LICENSES="MIT"
COPY bundle /catalogs
LABEL org.opencontainers.image.title="favicons-catalogs" \
      org.opencontainers.image.description="favicons-site with prerendered icon catalogs; each catalog keeps its own license, see /catalogs/NOTICE" \
      org.opencontainers.image.licenses="${LICENSES}" \
      org.opencontainers.image.source="https://github.com/peleda-app/favicons"
