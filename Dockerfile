FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked \
    && cp target/release/favicons /favicons \
    && mkdir -p /rootfs/run/favicons /rootfs/var/cache/favicons

FROM scratch
COPY --from=build /favicons /favicons
COPY --from=build --chown=65534:65534 /rootfs/ /
COPY config.example.toml /etc/favicons/config.toml
USER 65534:65534
ENV FAVICONS_CONFIG=/etc/favicons/config.toml
EXPOSE 8080
ENTRYPOINT ["/favicons"]
