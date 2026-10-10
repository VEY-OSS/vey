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
COPY vey-proxy/docker/config/main.yaml /etc/vey-proxy/main.yaml
EXPOSE 8080 1080
ENTRYPOINT ["/usr/bin/vey-proxy"]
CMD ["-c", "/etc/vey-proxy/", "-G", "default", "-v"]
