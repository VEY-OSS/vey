FROM rust:trixie AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apt-get update && apt-get install -y clang libclang-dev cmake capnproto
ENV CC=clang
ENV CXX=clang++
ARG VEY_PGO_PROFILE
ENV VEY_PGO_PROFILE=${VEY_PGO_PROFILE}
RUN scripts/package/cargo_with_pgo.sh build --profile release-lto \
 --no-default-features --features secure-snmalloc,vendored-boringssl,rustls-ring,quic,vendored-c-ares \
 -p vey-proxy -p vey-proxy-ctl -p vey-proxy-ftp

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy /usr/bin/vey-proxy
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy-ctl /usr/bin/vey-proxy-ctl
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy-ftp /usr/bin/vey-proxy-ftp
COPY vey-proxy/docker/config/main.yaml /etc/vey-proxy/main.yaml
LABEL org.opencontainers.image.title="vey-proxy" \
      org.opencontainers.image.description="HTTP and SOCKS proxy with transparent, stream, and reverse-proxy servers. The default image listens on 8080 and 1080." \
      org.opencontainers.image.url="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.source="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.documentation="https://github.com/VEY-OSS/vey/blob/main/vey-proxy/docker/README.md" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.vendor="VEY-OSS"
EXPOSE 8080 1080
ENTRYPOINT ["/usr/bin/vey-proxy"]
CMD ["-c", "/etc/vey-proxy/", "-G", "default", "-v"]
