FROM gcr.io/oss-fuzz-base/base-builder-rust

# base-builder-rust pins RUSTUP_TOOLCHAIN=nightly-2025-09-05 (rustc 1.91.0-nightly).
# Cargo rejects that compiler because these crates set rust-version = "1.95.0".
# Follow the nightly channel instead, and make it the default so the OSS-Fuzz
# compile script copies libstd sources from the same toolchain.
ENV RUSTUP_TOOLCHAIN=nightly
RUN rustup toolchain install nightly --profile minimal --component rust-src \
    && rustup default nightly

RUN apt-get update && apt-get install -y --no-install-recommends zip \
    && rm -rf /var/lib/apt/lists/*

COPY . $SRC/vey
WORKDIR $SRC/vey
COPY ./.clusterfuzzlite/build.sh $SRC/build.sh
