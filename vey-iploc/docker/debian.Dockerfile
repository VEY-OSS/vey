FROM rust:trixie AS builder
WORKDIR /usr/src/vey
COPY . .
RUN cargo build --profile release-lto -p vey-iploc

FROM debian:trixie-slim
COPY --from=builder /usr/src/vey/target/release-lto/vey-iploc /usr/bin/vey-iploc
LABEL org.opencontainers.image.title="vey-iploc" \
      org.opencontainers.image.description="IP geolocation lookup for vey-proxy." \
      org.opencontainers.image.url="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.source="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.documentation="https://github.com/VEY-OSS/vey/blob/main/vey-iploc/docker/README.md" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.vendor="VEY-OSS"
ENTRYPOINT ["/usr/bin/vey-iploc"]
CMD ["-Vvv"]
