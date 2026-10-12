# Rust Profile-Guided Optimization

Profile-guided optimization (PGO) compiles the Rust code twice. The first build is instrumented and run on a workload. The second build reads the merged profile and lays out the hot paths for that workload. Vendored C libraries such as OpenSSL and c-ares are not part of this profile.

The package that benefits is `vey-proxy`. `vey-mkcert` is a short-lived certificate tool and is not worth a profile.

Merging the raw profiles uses `cargo profdata` from `cargo-binutils`, the same tool the coverage scripts use. Setup is in [Development Environment Setup](dev-setup.md).

## Generate a profile

Run the component script. It builds an instrumented binary, runs that component's `scripts/testcases/<component>/run.sh`, and writes one `.profdata` file. Keep that file and point `VEY_PGO_PROFILE` at it when you build.

```shell
./scripts/pgo/vey-proxy.sh
```

The file is `scripts/pgo/profile/vey-proxy.profdata`. Set `VEY_PGO_OUT` to write it somewhere else:

```shell
VEY_PGO_OUT=/tmp/vey-proxy.profdata ./scripts/pgo/vey-proxy.sh
```

The instrumented build uses the `release-lto` profile and `target/pgo-generate`, so it matches packaged binaries and does not reuse objects from a normal `target/` build. Helper binaries are built into `target/debug` and are not part of the profile.

`vey-proxy` runs every scenario under `scripts/testcases/vey-proxy/`, including HTTP, SOCKS, TLS, FTP, and auth. The same scripts exist for the other daemons that already have testcases:

```shell
./scripts/pgo/vey-statsd.sh
./scripts/pgo/vey-keyless.sh
./scripts/pgo/vey-dcgen.sh
./scripts/pgo/vey-iploc.sh
```

`vey-dcgen` and `vey-iploc` reuse the proxy scenarios, with that daemon as the instrumented process. `vey-gateway` has no scenarios.

The proxy and statsd cases need Docker, `jq`, and the Python modules `pycurl` and `requests`. `scripts/coverage/vey-proxy.sh` rebuilds with coverage instrumentation (`-C instrument-coverage`); use `scripts/pgo/vey-proxy.sh` for a profile.

Generate the profile with the same `rustc` you will use for the optimized build. A profile from another compiler is rejected or ignored.

CI checks profile selection and runs one compiler round-trip (`vey-mkcert`: instrument, merge, build with the profile). A release tag (`vey-proxy-v*`, `vey-statsd-v*`, `vey-keyless-v*`, `vey-dcgen-v*`, `vey-iploc-v*`) runs that component's script on the tagged commit and uploads `scripts/pgo/profile/<component>.profdata` as the `pgo-<component>-<version>` artifact. The component scripts above are the workloads used for a real profile.

## Use the profile when packaging

The profile is a compile-time input. It is not installed into the package.

Deb, rpm, and Docker image builds call `scripts/package/cargo_with_pgo.sh`. That script adds `-Cprofile-use` only when a profile is present:

- `VEY_PGO_PROFILE` points at a `.profdata` file (absolute, or relative to the repository root), or
- `scripts/pgo/profile/<package>.profdata` is in the source tree for a package named in that `cargo build -p` / `--package` command

Each component has its own default file. A `vey-proxy` package build uses `scripts/pgo/profile/vey-proxy.profdata`; `vey-statsd` uses `scripts/pgo/profile/vey-statsd.profdata`. If one `cargo` command lists several packages, the first listed package that has a profile file is used. `VEY_PGO_PROFILE` wins over those files. With neither, the build is unchanged.

```shell
VEY_PGO_PROFILE=/tmp/vey-proxy.profdata ./scripts/build_deb_from_git.sh vey-proxy
VEY_PGO_PROFILE=/tmp/vey-proxy.profdata ./scripts/build_rpm_from_git.sh vey-proxy
```

The same variable applies to `build_deb_from_tar.sh` and `build_rpm_from_tar.sh`. Those scripts turn the path into an absolute path and pass it into the package build. An rpm build receives it as `vey_pgo_profile`; a deb build receives it as `VEY_PGO_PROFILE`.

`docker build` does not see the host environment, and a profile outside the build context is not in the image. `scripts/package/docker_build.sh` reads `VEY_PGO_PROFILE`, stages that file into the context when it lives outside the tree, and forwards every other argument to `docker build`:

```shell
VEY_PGO_PROFILE=/tmp/vey-proxy.profdata \
  ./scripts/package/docker_build.sh \
  -f vey-proxy/docker/debian.Dockerfile . -t vey-proxy:pgo
```

Leaving `scripts/pgo/profile/vey-proxy.profdata` in the build context is enough for the `vey-proxy` image. The same pattern applies to the other components. Pass another file with a build arg:

```shell
docker build \
  --build-arg VEY_PGO_PROFILE=scripts/pgo/profile/vey-proxy.profdata \
  -f vey-proxy/docker/debian.Dockerfile . -t vey-proxy:pgo
```

A local optimized build without the package scripts:

```shell
RUSTFLAGS="$(scripts/package/pgo_rustflags.sh -p vey-proxy)" \
  cargo build --profile release-lto -p vey-proxy
```

Functions the profile never saw are compiled as usual.
