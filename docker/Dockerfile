###########
# Builder #
###########
FROM rust:1-bullseye AS builder
WORKDIR /workspace
# Cache dependencies first
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main(){}" > src/main.rs && cargo build --release || true
# Now copy the full source and build
COPY . .
RUN cargo build --release

############
# Runtime  #
############
FROM debian:bookworm-slim

# Glibc is required for Factorio; include basic tools for download/extract
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tar xz-utils unzip \
    && rm -rf /var/lib/apt/lists/* \
    && update-ca-certificates

ENV FACTORIO_VERSION=stable \
    FSMR_BIND_ADDR=0.0.0.0:80

WORKDIR /opt/fsm

# Copy binary and static assets
COPY --from=builder /workspace/target/release/factorio-server-manager-rs /opt/fsm/factorio-server-manager-rs
COPY --from=builder /workspace/app/ /opt/fsm/app/
COPY --from=builder /workspace/conf.toml.example /opt/fsm/conf.toml.example
COPY ./entrypoint.sh /opt/entrypoint.sh

RUN chmod +x /opt/entrypoint.sh

VOLUME /opt/fsm-data /opt/factorio/saves /opt/factorio/mods /opt/factorio/config /opt/fsm/mod_packs

EXPOSE 80/tcp 34197/udp

ENTRYPOINT ["/opt/entrypoint.sh"]
