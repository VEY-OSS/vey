FROM rust:alpine AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apk add --no-cache musl-dev cmake capnproto-dev g++ linux-headers make
ENV RUSTFLAGS="-Ctarget-feature=-crt-static"
RUN cargo build --profile release-lto --features secure-snmalloc \
    -p vey-statsd -p vey-statsd-ctl

FROM alpine:latest
RUN apk add --no-cache libgcc ca-certificates
COPY --from=builder /usr/src/vey/target/release-lto/vey-statsd /usr/bin/vey-statsd
COPY --from=builder /usr/src/vey/target/release-lto/vey-statsd-ctl /usr/bin/vey-statsd-ctl
COPY vey-statsd/docker/config/main.yaml /etc/vey-statsd/main.yaml
EXPOSE 8125/udp
ENTRYPOINT ["/usr/bin/vey-statsd"]
CMD ["-c", "/etc/vey-statsd/", "-G", "default", "-v"]
