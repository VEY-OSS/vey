FROM rust:trixie AS builder
WORKDIR /usr/src/vey
COPY . .
RUN cargo build --profile release-lto --features vendored-openssl -p vey-dcgen

FROM debian:trixie-slim
COPY --from=builder /usr/src/vey/target/release-lto/vey-dcgen /usr/bin/vey-dcgen
LABEL org.opencontainers.image.title="vey-dcgen" \
      org.opencontainers.image.description="Dynamic certificate generator for vey-proxy TLS interception." \
      org.opencontainers.image.url="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.source="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.documentation="https://github.com/VEY-OSS/vey/blob/main/vey-dcgen/docker/README.md" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.vendor="VEY-OSS"
ENTRYPOINT ["/usr/bin/vey-dcgen"]
CMD ["-Vvv"]
