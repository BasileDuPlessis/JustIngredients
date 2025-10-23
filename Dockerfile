# Use Ubuntu as the base image for both build and runtime
FROM ubuntu:22.04 as builder

# Install system dependencies for leptonica and tesseract
RUN apt-get update && apt-get install -y \
    software-properties-common \
    && add-apt-repository ppa:alex-p/tesseract-ocr5 \
    && apt-get update && apt-get install -y \
    pkg-config \
    libtesseract-dev \
    libleptonica-dev \
    libclang-dev \
    build-essential \
    curl \
    libssl-dev \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    && . $HOME/.cargo/env \
    && rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /app

# Copy the Cargo.toml and Cargo.lock files
COPY Cargo.toml Cargo.lock ./

# Copy the source code
COPY src ./src

# Copy the locales directory
COPY locales ./locales

# Copy the config directory
COPY config ./config

# Build the application in release mode
RUN . $HOME/.cargo/env && cargo build --release

# Create a new stage for the runtime image
FROM ubuntu:22.04

# Install required dependencies for Tesseract and runtime (same as builder)
RUN apt-get update && apt-get install -y \
    software-properties-common \
    && add-apt-repository ppa:alex-p/tesseract-ocr5 \
    && apt-get update && apt-get install -y \
    tesseract-ocr \
    tesseract-ocr-eng \
    tesseract-ocr-fra \
    libtesseract5 \
    liblept5 \
    libtesseract-dev \
    libleptonica-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -r -s /bin/false justingredients

# Set the working directory
WORKDIR /app

# Copy the binary from the builder stage
COPY --from=builder /app/target/release/just-ingredients /app/just-ingredients

# Copy the config directory from the builder stage
COPY --from=builder /app/config ./config

# Copy the locales directory from the builder stage
COPY --from=builder /app/locales ./locales

# Change ownership of the binary and directories
RUN chown -R justingredients:justingredients /app/just-ingredients /app/config /app/locales

# Switch to the non-root user
USER justingredients

# Expose the port the app runs on
EXPOSE 8080

# Command to run the application
CMD ["./just-ingredients"]