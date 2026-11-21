# Images for the adapter end-to-end test. Build context: the repository root.
#
# `fangst` and `mocks` compile from source, which works on any host with Docker. CI builds the two
# binaries on the runner instead, where the Rust cache survives between runs, and uses the
# `*-prebuilt` targets, which copy them from tests/adapters/.bin.

FROM rust:1.95.0-trixie AS build
# The toolchain file asks for clippy and rustfmt, which a build doesn't need.
ENV RUSTUP_TOOLCHAIN=1.95.0
# Debug info only makes the images bigger.
ENV CARGO_PROFILE_DEV_DEBUG=0
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked -p fangst-dl -p e2e-mocks \
    && mkdir -p /out \
    && cp target/debug/fangst-dl target/debug/e2e-mocks /out/

FROM debian:trixie-20261104-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

FROM runtime AS fangst
COPY --from=build /out/fangst-dl /usr/local/bin/fangst-dl
USER 1000:1000
ENTRYPOINT ["fangst-dl"]

FROM runtime AS mocks
COPY --from=build /out/e2e-mocks /usr/local/bin/e2e-mocks
ENTRYPOINT ["e2e-mocks"]

FROM runtime AS fangst-prebuilt
COPY tests/adapters/.bin/fangst-dl /usr/local/bin/fangst-dl
USER 1000:1000
ENTRYPOINT ["fangst-dl"]

FROM runtime AS mocks-prebuilt
COPY tests/adapters/.bin/e2e-mocks /usr/local/bin/e2e-mocks
ENTRYPOINT ["e2e-mocks"]
