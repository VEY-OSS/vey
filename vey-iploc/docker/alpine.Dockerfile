FROM rust:alpine AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apk add --no-cache musl-dev
ENV RUSTFLAGS="-Ctarget-feature=-crt-static"
RUN cargo build --profile release-lto -p vey-iploc

FROM alpine:latest
RUN apk add --no-cache libgcc
COPY --from=builder /usr/src/vey/target/release-lto/vey-iploc /usr/bin/vey-iploc
COPY vey-iploc/docker/config/main.yaml /etc/vey-iploc/main.yaml
COPY vey-iploc/docker/config/country.csv /etc/vey-iploc/country.csv
ENV UDP_LISTEN_ADDR=0.0.0.0:2888
EXPOSE 2888/udp
ENTRYPOINT ["/usr/bin/vey-iploc"]
CMD ["-c", "/etc/vey-iploc/", "-G", "default", "-v"]
