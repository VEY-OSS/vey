FROM rust:alpine AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apk add --no-cache musl-dev openssl-dev
ENV RUSTFLAGS="-Ctarget-feature=-crt-static"
RUN cargo build --profile release-lto -p vey-dcgen

FROM alpine:latest
RUN apk add --no-cache libgcc
COPY --from=builder /usr/src/vey/target/release-lto/vey-dcgen /usr/bin/vey-dcgen
ENTRYPOINT ["/usr/bin/vey-dcgen"]
CMD ["-Vvv"]
