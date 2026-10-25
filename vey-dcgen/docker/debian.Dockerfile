FROM rust:trixie AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apt-get update && apt-get install -y cmake g++ \
    && rm -rf /var/lib/apt/lists/*
ARG VEY_PGO_PROFILE
ENV VEY_PGO_PROFILE=${VEY_PGO_PROFILE}
RUN scripts/package/cargo_with_pgo.sh build --profile release-lto --features secure-snmalloc,vendored-openssl -p vey-dcgen -p vey-mkcert

FROM debian:trixie-slim
COPY --from=builder /usr/src/vey/target/release-lto/vey-dcgen /usr/bin/vey-dcgen
COPY --from=builder /usr/src/vey/target/release-lto/vey-mkcert /usr/bin/vey-mkcert
COPY vey-dcgen/docker/config/main.yaml /etc/vey-dcgen/main.yaml
COPY vey-dcgen/docker/entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod 755 /usr/local/bin/docker-entrypoint.sh && mkdir -p /var/lib/vey-dcgen
LABEL org.opencontainers.image.title="vey-dcgen" \
      org.opencontainers.image.description="Dynamic certificate generator for vey-proxy TLS interception. The default image listens on UDP 2999 and creates a root CA on first start." \
      org.opencontainers.image.url="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.source="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.documentation="https://github.com/VEY-OSS/vey/blob/main/vey-dcgen/docker/README.md" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.vendor="VEY-OSS"
ENV UDP_LISTEN_ADDR=0.0.0.0:2999
EXPOSE 2999/udp
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["-c", "/etc/vey-dcgen/", "-G", "default", "-v"]
