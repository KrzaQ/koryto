# syntax=docker/dockerfile:1

# Stage 1: the Vue bundle.
FROM node:24-bookworm-slim AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# Stage 2: the binary, with the bundle embedded by rust-embed.
FROM rust:1.96-bookworm AS backend
WORKDIR /app
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY migrations ./migrations
COPY --from=frontend /app/frontend/dist ./frontend/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && cp target/release/koryto /usr/local/bin/koryto

# Stage 3: runtime.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=backend /usr/local/bin/koryto /usr/local/bin/koryto
RUN useradd --system --uid 10001 --create-home koryto
USER koryto
ENV KORYTO_BIND=0.0.0.0:8000
EXPOSE 8000
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s \
    CMD curl -fsS http://127.0.0.1:8000/api/health || exit 1
ENTRYPOINT ["koryto"]
CMD ["serve"]
