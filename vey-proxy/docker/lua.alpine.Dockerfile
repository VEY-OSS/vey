FROM nickblah/lua:5.4-luarocks-alpine3.22 AS lua
RUN apk add --no-cache build-base
RUN luarocks install lua-cjson

FROM rust:alpine3.22 AS builder
WORKDIR /usr/src/vey
COPY . .
RUN apk add --no-cache musl-dev linux-headers libc++-dev libc++-static llvm-libunwind-static \
    clang lld clang-libclang cmake make capnproto-dev c-ares-dev lua5.4-dev pkgconfig
ENV PKG_CONFIG_PATH=/usr/lib/pkgconfig
ENV CC=clang
ENV CXX=clang++
ENV CXXSTDLIB=c++
ENV CXXFLAGS=-stdlib=libc++
# Same linker setup as the musl-clang-libcxx static build, without crt-static.
ENV RUSTFLAGS="-Ctarget-feature=-crt-static -Clinker=clang -Clink-arg=-fuse-ld=lld -Clink-arg=-lc++abi"
ARG VEY_PGO_PROFILE
ENV VEY_PGO_PROFILE=${VEY_PGO_PROFILE}
RUN scripts/package/cargo_with_pgo.sh build --profile release-lto \
  --no-default-features --features secure-vendored-mimalloc,vendored-boringssl,rustls-ring,quic,c-ares,lua54 \
  -p vey-proxy -p vey-proxy-ctl -p vey-proxy-lua

FROM alpine:3.22
RUN apk add --no-cache ca-certificates libgcc libc++ c-ares lua5.4-libs
COPY --from=lua /usr/local/bin/lua /usr/local/bin/
COPY --from=lua /usr/local/bin/luarocks /usr/local/bin/
COPY --from=lua /usr/local/lib/lua /usr/local/lib/
COPY --from=lua /usr/local/share/lua /usr/local/share/
COPY --from=lua /usr/local/lib/luarocks /usr/local/lib/luarocks
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy /usr/bin/
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy-ctl /usr/bin/
COPY --from=builder /usr/src/vey/target/release-lto/vey-proxy-lua /usr/bin/
COPY vey-proxy/docker/config/main.yaml /etc/vey-proxy/main.yaml
LABEL org.opencontainers.image.title="vey-proxy" \
      org.opencontainers.image.description="Forward, reverse, and transparent proxy with TLS interception and policy controls. The default image listens on 8080 and 1080." \
      org.opencontainers.image.url="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.source="https://github.com/VEY-OSS/vey" \
      org.opencontainers.image.documentation="https://github.com/VEY-OSS/vey/blob/main/vey-proxy/docker/README.md" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.vendor="VEY-OSS"
EXPOSE 8080 1080
ENTRYPOINT ["/usr/bin/vey-proxy"]
CMD ["-c", "/etc/vey-proxy/", "-G", "default", "-v"]
