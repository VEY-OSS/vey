FROM rust:alpine AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apk add --no-cache musl-dev cmake capnproto-dev openssl-dev c-ares-dev
ENV RUSTFLAGS="-Ctarget-feature=-crt-static"
RUN cargo build --profile release-lto \
 --no-default-features --features rustls-ring,quic,c-ares \
 -p vey-proxy -p vey-proxy-ctl

FROM alpine:latest
RUN apk add --no-cache libgcc c-ares
RUN apk add --no-cache ca-certificates
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy /usr/bin/vey-proxy
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy-ctl /usr/bin/vey-proxy-ctl
LABEL org.opencontainers.image.title="vey-proxy" \
      org.opencontainers.image.description="HTTP and SOCKS proxy with transparent, stream, and reverse-proxy servers." \
      org.opencontainers.image.url="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.source="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.documentation="https://github.com/VEY-OSS/vey/blob/main/vey-proxy/docker/README.md" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.vendor="VEY-OSS"
ENTRYPOINT ["/usr/bin/vey-proxy"]
CMD ["-Vvv"]
