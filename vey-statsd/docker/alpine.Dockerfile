FROM rust:alpine AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apk add --no-cache musl-dev cmake capnproto-dev clang make
ENV RUSTFLAGS="-Ctarget-feature=-crt-static"
ARG VEY_PGO_PROFILE
ENV VEY_PGO_PROFILE=${VEY_PGO_PROFILE}
RUN scripts/package/cargo_with_pgo.sh build --profile release-lto --features secure-vendored-mimalloc \
    -p vey-statsd -p vey-statsd-ctl

FROM alpine:latest
RUN apk add --no-cache libgcc ca-certificates
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
