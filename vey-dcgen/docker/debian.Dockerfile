FROM rust:bookworm AS builder
WORKDIR /usr/src/vey
COPY . .
RUN cargo build --profile release-lto --features vendored-openssl -p vey-dcgen

FROM debian:bookworm-slim
COPY --from=builder /usr/src/vey/target/release-lto/vey-dcgen /usr/bin/vey-dcgen
ENTRYPOINT ["/usr/bin/vey-dcgen"]
CMD ["-Vvv"]
