# syntax=docker/dockerfile:1

# ---- builder ----
FROM rust:1.91-slim AS builder
WORKDIR /app
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
CMD ["stonkscollect-backend"]
