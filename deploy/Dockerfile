# syntax=docker/dockerfile:1.6

# Builder and runtime use the same Debian release so the binary links against the
# exact library versions installed at runtime. Bump both together.
FROM rust:1.93-slim-trixie AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       build-essential \
       clang \
       pkg-config \
       libssl-dev \
       libtesseract-dev \
       libleptonica-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY config ./config
COPY locales ./locales
COPY .sqlx ./.sqlx

RUN cargo build --release \
    && strip --strip-unneeded target/release/just-ingredients

FROM debian:trixie-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       ca-certificates \
       libssl3t64 \
       libleptonica6 \
       libtesseract5 \
       tesseract-ocr \
       tesseract-ocr-eng \
       tesseract-ocr-fra \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/just-ingredients /usr/local/bin/just-ingredients
COPY config ./config

# Fail the build instead of shipping an image whose binary cannot start.
RUN ! ldd /usr/local/bin/just-ingredients | grep 'not found'

ENV MEASUREMENT_UNITS_CONFIG_PATH=/app/config/measurement_units.json
ENV RUST_LOG=info,sqlx=warn

CMD ["just-ingredients"]
