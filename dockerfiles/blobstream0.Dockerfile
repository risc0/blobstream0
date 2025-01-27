# Use the official Rust image as the base image
FROM rust:1.81-bullseye as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    libssl-dev \
    pkg-config

# Install Foundry
RUN curl -L https://foundry.paradigm.xyz | bash
ENV PATH="/root/.foundry/bin:${PATH}"
RUN foundryup

# Install cargo risczero
RUN curl -L https://risczero.com/install | bash
ENV PATH="/root/.risc0/bin:${PATH}"
RUN rzup install

# Create and set permissions for the /app directory
# RUN mkdir -p /app && chown -R nobody:nobody /app

# Set the working directory to /app
WORKDIR /app

# Copy the entire project
COPY . .

# Build the project
RUN cargo build -p blobstream0 --release --features prebuilt-docker,fireblocks

# Create a new stage for a smaller final image
FROM debian:bullseye-slim as final

# Install necessary runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy the built binary from the builder stage
COPY --from=builder /app/target/release/blobstream0 /usr/local/bin/blobstream0

# Set the entrypoint to the blobstream0 CLI
ENTRYPOINT ["blobstream0"]
