FROM scratch
COPY sites /sites
LABEL org.opencontainers.image.title="favicons-sites" \
      org.opencontainers.image.description="Prerendered favicons of the 100,000 most popular sites, a layer of the favicons.page image; sources and licenses in /sites/SOURCES.md" \
      org.opencontainers.image.licenses="CC-BY-SA-4.0 AND CC-BY-3.0" \
      org.opencontainers.image.source="https://github.com/peleda-app/favicons"
