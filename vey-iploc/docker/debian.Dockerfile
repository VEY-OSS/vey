FROM rust:trixie AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apt-get update && apt-get install -y cmake g++ clang pkg-config libbpf-dev \
    && rm -rf /var/lib/apt/lists/*
ARG VEY_PGO_PROFILE
ENV VEY_PGO_PROFILE=${VEY_PGO_PROFILE}
RUN scripts/package/cargo_with_pgo.sh build --profile release-lto --features secure-snmalloc,ebpf -p vey-iploc

FROM debian:trixie-slim
COPY --from=builder /usr/src/vey/target/release-lto/vey-iploc /usr/bin/vey-iploc
COPY vey-iploc/docker/config/main.yaml /etc/vey-iploc/main.yaml
COPY vey-iploc/docker/config/country.csv /etc/vey-iploc/country.csv
LABEL org.opencontainers.image.title="vey-iploc" \
      org.opencontainers.image.description="IP-to-country and ASN lookup for vey-proxy GeoIP routing. The default image listens on UDP 2888. Replace the placeholder country database before using it for routing." \
      org.opencontainers.image.url="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.source="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.documentation="https://github.com/VEY-OSS/vey/blob/main/vey-iploc/docker/README.md" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.vendor="VEY-OSS"
EXPOSE 2888/udp
ENTRYPOINT ["/usr/bin/vey-iploc"]
CMD ["-c", "/etc/vey-iploc/", "-v"]
