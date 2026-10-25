# syntax=docker/dockerfile:1.7
FROM rust:1-bookworm AS build
ARG TRUNK_VERSION=0.21.14
ARG TARGETARCH
# No distro binaryen here on purpose: trunk downloads the wasm-opt pinned in crates/web/Trunk.toml.
RUN rustup target add wasm32-unknown-unknown \
 && arch="$( [ "$(uname -m)" = aarch64 ] && echo aarch64 || echo x86_64 )" \
 && curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-${arch}-unknown-linux-gnu.tar.gz" \
    | tar -xz -C /usr/local/bin trunk \
 && trunk --version
WORKDIR /src
COPY . .
# Cache mounts keep dependency builds warm across image builds (BuildKit).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release -p wartide-server \
 && cp target/release/wartide-server /wartide-server
# Fails loudly if crates/web/index.html or Trunk.toml are missing.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cd crates/web && trunk build --release \
 && cp -r dist /dist

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /app wartide
WORKDIR /app
COPY --from=build /wartide-server /app/wartide-server
COPY --from=build /dist /app/dist
ENV WEB_DIST_DIR=/app/dist PORT=8080
USER wartide
EXPOSE 8080
CMD ["/app/wartide-server"]
