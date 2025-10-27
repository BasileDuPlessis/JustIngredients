# syntax=docker/dockerfile:1.6

# syntax=docker/dockerfile:1.6

FROM rust:latest AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       build-essential \
       clang \
       pkg-config \
       libssl-dev \
       libtesseract-dev \
       libleptonica-dev \
       tesseract-ocr-eng \
       tesseract-ocr-fra \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY config ./config
COPY locales ./locales

RUN cargo build --release \
    && strip --strip-unneeded target/release/just-ingredients

FROM ubuntu:latest AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       ca-certificates \
       libssl3 \
       tesseract-ocr \
       tesseract-ocr-eng \
       tesseract-ocr-fra \
       liblept5 \
       libtesseract5 \
    && ln -s /usr/lib/aarch64-linux-gnu/liblept.so.5 /usr/lib/aarch64-linux-gnu/libleptonica.so.6 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/just-ingredients /usr/local/bin/just-ingredients
COPY config ./config

ENV MEASUREMENT_UNITS_CONFIG_PATH=/app/config/measurement_units.json
ENV RUST_LOG=info,sqlx=warn

CMD ["just-ingredients"]
