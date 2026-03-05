FROM rust:alpine AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apk add --no-cache musl-dev
ENV RUSTFLAGS="-Ctarget-feature=-crt-static"
RUN cargo build --profile release-lto -p vey-iploc

FROM alpine:latest
RUN apk add --no-cache libgcc
COPY --from=builder /usr/src/vey/target/release-lto/vey-iploc /usr/bin/vey-iploc
ENTRYPOINT ["/usr/bin/vey-iploc"]
CMD ["-Vvv"]
