FROM rust:alpine AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apk add --no-cache musl-dev linux-headers libc++-dev libc++-static llvm-libunwind-static \
    clang lld clang-libclang cmake make capnproto-dev c-ares-dev
ENV CC=clang
ENV CXX=clang++
ENV CXXSTDLIB=c++
ENV CXXFLAGS=-stdlib=libc++
# Same linker setup as the musl-clang-libcxx static build, without crt-static.
ENV RUSTFLAGS="-Ctarget-feature=-crt-static -Clinker=clang -Clink-arg=-fuse-ld=lld -Clink-arg=-lc++abi"
ARG VEY_PGO_PROFILE
ENV VEY_PGO_PROFILE=${VEY_PGO_PROFILE}
RUN scripts/package/cargo_with_pgo.sh build --profile release-lto \
 --no-default-features --features secure-vendored-mimalloc,vendored-boringssl,rustls-ring,quic,c-ares \
 -p vey-proxy -p vey-proxy-ctl

FROM alpine:latest
RUN apk add --no-cache libgcc libc++ c-ares
RUN apk add --no-cache ca-certificates
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy /usr/bin/vey-proxy
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy-ctl /usr/bin/vey-proxy-ctl
COPY vey-proxy/docker/config/main.yaml /etc/vey-proxy/main.yaml
EXPOSE 8080 1080
ENTRYPOINT ["/usr/bin/vey-proxy"]
CMD ["-c", "/etc/vey-proxy/", "-G", "default", "-v"]
