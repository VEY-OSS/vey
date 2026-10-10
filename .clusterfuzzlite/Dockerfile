FROM gcr.io/oss-fuzz-base/base-builder-rust

RUN apt-get update && apt-get install -y --no-install-recommends zip \
    && rm -rf /var/lib/apt/lists/*

COPY . $SRC/vey
WORKDIR $SRC/vey
COPY ./.clusterfuzzlite/build.sh $SRC/build.sh
