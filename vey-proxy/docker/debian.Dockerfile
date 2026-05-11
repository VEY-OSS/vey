FROM rust:trixie AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apt-get update && apt-get install -y clang libclang-dev cmake capnproto
ENV CC=clang
ENV CXX=clang++
RUN cargo build --profile release-lto \
 --no-default-features --features vendored-boringssl,rustls-ring,quic,vendored-c-ares \
 -p vey-proxy -p vey-proxy-ctl -p vey-proxy-ftp

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy /usr/bin/vey-proxy
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy-ctl /usr/bin/vey-proxy-ctl
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy-ftp /usr/bin/vey-proxy-ftp
ENTRYPOINT ["/usr/bin/vey-proxy"]
CMD ["-Vvv"]
