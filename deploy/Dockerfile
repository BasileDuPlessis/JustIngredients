# syntax=docker/dockerfile:1.6

# Builder and runtime use the same Debian release so the binary links against the
# exact library versions installed at runtime. Bump both together, and keep the Rust
# version in sync with rust-toolchain.toml.
FROM rust:1.99-slim-trixie AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       build-essential \
       pkg-config \
       libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./

# Dependencies first, against placeholder sources: this layer is reused (CI layer cache) as long
# as Cargo.toml and Cargo.lock do not change, so a code change only rebuilds the crate itself.
RUN mkdir -p src examples \
    && echo 'fn main() {}' > src/main.rs \
    && touch src/lib.rs \
    && echo 'fn main() {}' > examples/recipe_parser.rs \
    && cargo build --release \
    && rm -rf src examples

COPY src ./src
COPY config ./config
COPY locales ./locales
COPY .sqlx ./.sqlx

# COPY keeps the files' times, which can be older than the placeholder build: touch them so
# cargo rebuilds the crate.
RUN touch src/main.rs src/lib.rs \
    && cargo build --release \
    && strip --strip-unneeded target/release/just-ingredients

FROM debian:trixie-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       ca-certificates \
       libssl3t64 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/just-ingredients /usr/local/bin/just-ingredients
COPY config ./config

# Fail the build instead of shipping an image whose binary cannot start.
RUN ! ldd /usr/local/bin/just-ingredients | grep 'not found'

ENV MEASUREMENT_UNITS_CONFIG_PATH=/app/config/measurement_units.json
ENV RUST_LOG=info,sqlx=warn

CMD ["just-ingredients"]
