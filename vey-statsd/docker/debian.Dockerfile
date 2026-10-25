FROM rust:trixie AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apt-get update && apt-get install -y capnproto cmake g++ \
    && rm -rf /var/lib/apt/lists/*
ARG VEY_PGO_PROFILE
ENV VEY_PGO_PROFILE=${VEY_PGO_PROFILE}
RUN scripts/package/cargo_with_pgo.sh build --profile release-lto --features secure-snmalloc \
    -p vey-statsd -p vey-statsd-ctl

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/vey/target/release-lto/vey-statsd /usr/bin/vey-statsd
COPY --from=builder /usr/src/vey/target/release-lto/vey-statsd-ctl /usr/bin/vey-statsd-ctl
COPY vey-statsd/docker/config/main.yaml /etc/vey-statsd/main.yaml
LABEL org.opencontainers.image.title="vey-statsd" \
      org.opencontainers.image.description="StatsD ingestion and export. The default image listens on UDP 8125 and prints aggregated metrics to stdout." \
      org.opencontainers.image.url="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.source="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.documentation="https://github.com/VEY-OSS/vey/blob/main/vey-statsd/docker/README.md" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.vendor="VEY-OSS"
EXPOSE 8125/udp
ENTRYPOINT ["/usr/bin/vey-statsd"]
CMD ["-c", "/etc/vey-statsd/", "-G", "default", "-v"]
