FROM rust:alpine AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apk add --no-cache musl-dev openssl-dev
ENV RUSTFLAGS="-Ctarget-feature=-crt-static"
RUN cargo build --profile release-lto -p vey-dcgen -p vey-mkcert

FROM alpine:latest
RUN apk add --no-cache libgcc libssl3
COPY --from=builder /usr/src/vey/target/release-lto/vey-dcgen /usr/bin/vey-dcgen
COPY --from=builder /usr/src/vey/target/release-lto/vey-mkcert /usr/bin/vey-mkcert
COPY vey-dcgen/docker/config/main.yaml /etc/vey-dcgen/main.yaml
COPY vey-dcgen/docker/entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod 755 /usr/local/bin/docker-entrypoint.sh && mkdir -p /var/lib/vey-dcgen
ENV UDP_LISTEN_ADDR=0.0.0.0:2999
EXPOSE 2999/udp
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["-c", "/etc/vey-dcgen/", "-G", "default", "-v"]
