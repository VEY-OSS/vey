#!/bin/bash -eu

cd "$SRC/vey"

cargo fuzz build -O --debug-assertions

if ! HOST_TUPLE="$(rustc --print host-tuple 2>/dev/null)"; then
    HOST_TUPLE="$(rustc -vV | awk '/^host: / { print $2 }')"
fi
FUZZ_TARGET_OUTPUT_DIR="fuzz/target/${HOST_TUPLE}/release"

for f in fuzz/fuzz_targets/*.rs; do
    name="$(basename "${f%.*}")"
    cp "${FUZZ_TARGET_OUTPUT_DIR}/${name}" "$OUT/"
    if [ -d "fuzz/corpus/${name}" ]; then
        zip -q -j "$OUT/${name}_seed_corpus.zip" "fuzz/corpus/${name}/"*
    fi
done
