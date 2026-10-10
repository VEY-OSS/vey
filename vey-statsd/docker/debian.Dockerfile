FROM rust:trixie AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apt-get update && apt-get install -y capnproto cmake g++ \
    && rm -rf /var/lib/apt/lists/*
RUN cargo build --profile release-lto --features secure-snmalloc \
    -p vey-statsd -p vey-statsd-ctl

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/vey/target/release-lto/vey-statsd /usr/bin/vey-statsd
COPY --from=builder /usr/src/vey/target/release-lto/vey-statsd-ctl /usr/bin/vey-statsd-ctl
COPY vey-statsd/docker/config/main.yaml /etc/vey-statsd/main.yaml
EXPOSE 8125/udp
ENTRYPOINT ["/usr/bin/vey-statsd"]
CMD ["-c", "/etc/vey-statsd/", "-G", "default", "-v"]
