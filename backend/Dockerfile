# syntax=docker/dockerfile:1

# ---- builder ----
FROM rust:1.91-slim AS builder
WORKDIR /app
# libsql-ffi (bundled SQLite), ring and zstd-sys compile C — rust:slim ships no
# C toolchain, so install one (build stage only; discarded in the runtime image).
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential clang libclang-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
# Cache deps: copy manifests, build a stub, then the real sources.
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src \
    && echo 'fn main() {}' > src/main.rs \
    && echo '' > src/lib.rs \
    && cargo build --release \
    && rm -rf src
COPY . .
RUN touch src/main.rs src/lib.rs && cargo build --release

# ---- runtime ----
FROM debian:bookworm-slim AS runtime
RUN useradd --system --create-home app
COPY --from=builder /app/target/release/stonkscollect-backend /usr/local/bin/stonkscollect-backend
USER app
ENV PORT=8080
EXPOSE 8080
# Default to serving the API; override for `bootstrap`/`collect`, e.g.
#   docker compose run --rm backend stonkscollect-backend collect --all
CMD ["stonkscollect-backend", "serve"]
