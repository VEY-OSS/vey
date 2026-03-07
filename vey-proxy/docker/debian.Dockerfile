FROM rust:bookworm AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apt-get update && apt-get install -y libclang-dev cmake capnproto
RUN cargo build --profile release-lto \
 --no-default-features --features vendored-boringssl,rustls-ring,quic,vendored-c-ares \
 -p vey-proxy -p vey-proxy-ctl

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy /usr/bin/vey-proxy
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy-ctl /usr/bin/vey-proxy-ctl
ENTRYPOINT ["/usr/bin/vey-proxy"]
CMD ["-Vvv"]
