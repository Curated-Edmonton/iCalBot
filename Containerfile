# ---- Builder Stage ----
# Use the official Rust image. The 'slim' variant is smaller.
FROM rust:slim-bookworm AS builder

# Set the working directory
WORKDIR /usr/src/app

# Install musl tools for static compilation
RUN apt-get update && apt-get install -y musl-tools
RUN rustup target add x86_64-unknown-linux-musl

# Copy your application's source code
COPY . .

# Build a statically-linked, release-optimized binary
# This command strips debug symbols and optimizes for size.
RUN cargo build --target x86_64-unknown-linux-musl --release

# ---- Final Stage ----
# Use the 'scratch' image, which is completely empty.
FROM scratch

ENV BINNAME="icalbot"

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-musl/release/$BINNAME /bot

# Set the entrypoint for the container
CMD ["/bot"]
