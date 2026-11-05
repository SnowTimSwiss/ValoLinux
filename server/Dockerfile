# ValoLinux management server. Build from the repository root:
#   docker build -f server/Dockerfile -t valolinux-server .
FROM rust:1-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY server server
COPY agent agent
RUN cargo build --release --locked -p valo-server

FROM debian:trixie-slim
RUN useradd --system --uid 10001 --home /data valo \
 && mkdir -p /data && chown valo /data
COPY --from=build /src/target/release/valo-server /usr/local/bin/valo-server
USER valo
ENV VALO_DATA_DIR=/data VALO_LISTEN=0.0.0.0:8080
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD ["/usr/local/bin/valo-server", "--healthcheck"]
CMD ["/usr/local/bin/valo-server"]
