# Use the official Rust image as the base image
FROM rust:latest as builder

# Install system dependencies for leptonica and tesseract
RUN apt-get update && apt-get install -y \
    pkg-config \
    libtesseract-dev \
    libleptonica-dev \
    libclang-dev \
    && rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /app

# Copy the Cargo.toml and Cargo.lock files
COPY Cargo.toml Cargo.lock ./

# Copy the source code
COPY src ./src

# Copy the locales directory
COPY locales ./locales

# Build the application in release mode
RUN cargo build --release

# Create a new stage for the runtime image
FROM debian:bookworm-slim

# Install required dependencies for Tesseract and runtime
RUN apt-get update && apt-get install -y \
    tesseract-ocr \
    tesseract-ocr-eng \
    tesseract-ocr-fra \
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

# Change ownership of the binary
RUN chown justingredients:justingredients /app/just-ingredients

# Switch to the non-root user
USER justingredients

# Expose the port the app runs on
EXPOSE 8080

# Command to run the application
CMD ["./just-ingredients"]