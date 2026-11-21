FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
ARG FEATURES=""
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY site ./site
RUN cargo build --release --locked --features "$FEATURES" \
    && cp target/release/favicons /favicons \
    && mkdir -p /rootfs/run/favicons /rootfs/var/cache/favicons

FROM scratch
COPY --from=build /favicons /favicons
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build --chown=65534:65534 /rootfs/ /
COPY config.example.toml /etc/favicons/config.toml
USER 65534:65534
ENV FAVICONS_CONFIG=/etc/favicons/config.toml \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
EXPOSE 8080
ENTRYPOINT ["/favicons"]
