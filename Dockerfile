# Multi-stage build (spec §12): build the frontend, build the server, then ship
# a small distroless runtime containing just the binary and the static assets.

# 1. Frontend — produce web/dist.
FROM node:22-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# 2. Server — build the release binary.
FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml ./
COPY server/ ./server/
RUN cargo build --release -p syncpad-server
# Stage a data directory owned by the distroless nonroot user (uid 65532), so a
# fresh named volume mounted at /data inherits writable ownership.
RUN mkdir -p /out/data && chown -R 65532:65532 /out/data

# 3. Runtime — distroless, non-root, no shell.
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=server /src/target/release/syncpad-server /app/syncpad-server
COPY --from=web /web/dist /app/web/dist
COPY --from=server --chown=65532:65532 /out/data /data

ENV SYNCPAD_STATIC_DIR=/app/web/dist \
    SYNCPAD_DATA_DIR=/data \
    PORT=8090 \
    RUST_LOG=info

EXPOSE 8090
VOLUME ["/data"]
ENTRYPOINT ["/app/syncpad-server"]
