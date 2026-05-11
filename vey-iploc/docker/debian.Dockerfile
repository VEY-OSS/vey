FROM rust:trixie AS builder
WORKDIR /usr/src/vey
COPY . .
RUN cargo build --profile release-lto -p vey-iploc

FROM debian:trixie-slim
COPY --from=builder /usr/src/vey/target/release-lto/vey-iploc /usr/bin/vey-iploc
ENTRYPOINT ["/usr/bin/vey-iploc"]
CMD ["-Vvv"]
