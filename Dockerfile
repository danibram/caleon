# Default build: pure Rust (local UDP protocol), any architecture:
#   docker build -t caleon .            (or --platform linux/arm64, linux/amd64...)
# With the optional Nabto SDK transport (linux/amd64 only):
#   docker build --platform linux/amd64 --build-arg FEATURES=nabto-sdk -t caleon .
FROM node:22-bookworm AS web
WORKDIR /web
COPY web/package.json web/pnpm-lock.yaml* ./
RUN corepack enable && pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

FROM rust:1.91-bookworm AS build
ARG FEATURES=""
ARG TARGETARCH
WORKDIR /src
COPY . .
COPY --from=web /web/dist ./web/dist
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=caleon-target-${TARGETARCH},target=/src/target \
    cargo build --release -p caleon-server ${FEATURES:+--features $FEATURES} && cp target/release/caleon /caleon

FROM debian:bookworm-slim
COPY --from=build /caleon /usr/local/bin/caleon
ENV DATA_DIR=/data BIND=0.0.0.0:8088
VOLUME /data
EXPOSE 8088
CMD ["caleon"]
