# syntax=docker/dockerfile:1
# Build from the repo root: docker build -f deploy/docker/Dockerfile -t fangst-dl .

# Keep the Rust tag in step with rust-toolchain.toml; that file is left out of the context so
# rustup doesn't try to download the clippy and rustfmt components it lists.
FROM rust:1.100.0-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY tools tools
# The cache mounts only speed up local rebuilds; the binary is copied out because the mount is
# gone once the step ends.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release --locked -p fangst-dl \
    && install -D -m 0755 target/release/fangst-dl /out/fangst-dl

FROM debian:trixie-slim
# tzdata makes TZ work; setpriv (util-linux, already in the base) drops root in the entrypoint.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /data /downloads \
    && chown 1000:1000 /data /downloads

COPY --from=build /out/fangst-dl /usr/local/bin/fangst-dl
COPY --chmod=0755 deploy/docker/entrypoint.sh /usr/local/bin/entrypoint
COPY --chmod=0755 deploy/docker/healthcheck.sh /usr/local/bin/healthcheck

ENV PUID=1000 \
    PGID=1000 \
    TZ=Etc/UTC

VOLUME ["/data", "/downloads"]
# 7740 is the API; 6881 is the torrent peer port, UDP for DHT.
EXPOSE 7740/tcp 6881/tcp 6881/udp

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --start-interval=2s --retries=3 \
    CMD ["healthcheck"]

LABEL org.opencontainers.image.title="fangst-dl" \
      org.opencontainers.image.description="The Fangst download daemon for torrent and usenet" \
      org.opencontainers.image.source="https://github.com/Fangst-Media/fangst" \
      org.opencontainers.image.licenses="AGPL-3.0-or-later"

# Starts as root only to apply PUID and PGID; the daemon itself never runs as root.
ENTRYPOINT ["entrypoint"]
