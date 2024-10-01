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

RUN cargo install cargo-binstall --version '=1.6.9' --locked
RUN cargo binstall cargo-risczero@1.1.1 --no-confirm --force
RUN cargo risczero install

# Create and set permissions for the /app directory
# RUN mkdir -p /app && chown -R nobody:nobody /app

# Set the working directory to /app
WORKDIR /app

# Copy the entire project
COPY . .

RUN cargo c -p light-client-guest

# Build the project
RUN cargo build -p blobstream0

run cp target/debug/blobstream0 /usr/local/bin/blobstream0

# Set the entrypoint to the blobstream0 CLI
ENTRYPOINT ["blobstream0"]
