# Development Setup

This document describes the baseline development environment for the VEY workspace.

## Rust Toolchain

The workspace requires Rust `1.90.0` or newer. Use the latest stable toolchain unless you have a specific reason not to.

### Install `rustup`

Install `rustup` from [rustup.rs](https://rustup.rs/). A non-root installation is recommended.

`cargo`, `rustc`, `rustup`, and related binaries are typically installed into `$HOME/.cargo/bin`. Make sure that
directory is in your `PATH`.

#### Bash

The installer usually adds this line to `$HOME/.profile`:

```sh
source "$HOME/.cargo/env"
```

#### Fish

Run:

```sh
set -U fish_user_paths $HOME/.cargo/bin $fish_user_paths
```

### Update `rustup`

```sh
rustup self update
```

### Install the stable toolchain and components

List available components:

```sh
rustup component list
```

The project expects these components to be installed:

- `rustc`
- `rust-std`
- `cargo`
- `rustfmt`
- `clippy`

`llvm-tools` is recommended:

```sh
rustup component add llvm-tools
```

### Install a nightly toolchain

Nightly is optional for normal builds, but useful for some IDE workflows and tools such as `cargo-expand`.

```sh
rustup toolchain install nightly
rustup component list --toolchain nightly
```

### Update installed toolchains

```sh
rustup update
```

## Recommended Cargo Tools

Install a cargo subcommand with:

```sh
cargo install --locked <crate-name>
```

Update an installed subcommand with:

```sh
cargo install --locked --force <crate-name>
```

Recommended tools:

- `cargo-expand`: useful for macro expansion in IDEs and debugging; typically used with nightly
- `cargo-audit`: audits `Cargo.lock` for known vulnerabilities
- `cargo-binutils`: exposes LLVM utilities installed through `llvm-tools`
- `cargo-cache`: manages Cargo caches
- `cargo-deny`: checks dependency licenses and supply-chain policy

## System Dependencies

### Core build dependencies

Most builds need:

- a C toolchain such as `gcc` or `clang`
- `make`
- `pkg-config` or `pkgconf`
- `capnproto`
- OpenSSL development headers

Some targets or features also need:

- `c-ares`
- Lua development headers if you enable a Lua feature such as `lua54`, `lua55`, or `luajit`
- Python development headers if you enable the default `python` feature
- `cmake` when building vendored `c-ares`

### Project feature notes

`vey-proxy` enables these features by default:

- `python`
- `c-ares`
- `quic`
- `rustls-ring`

If your platform does not provide all native dependencies, disable the unsupported defaults or use vendored features
where appropriate.

Example minimal build without Lua and Python:

```sh
cargo build --no-default-features --features rustls-ring,quic,c-ares
```

## Platform Guides

### Debian-based Linux

Debian or Ubuntu is the recommended development platform.

```sh
apt-get install gcc pkgconf make capnproto
apt-get install curl git jq tar xz-utils
apt-get install libssl-dev libc-ares-dev
# install a Lua dev package that exists on your distro, for example lua5.4 or lua5.5
apt-get install lua5.4-dev
apt-get install libpython3-dev
apt-get install python3-toml python3-requests python3-pycurl python3-semver python3-socks python3-dnspython
apt-get install python3-maxminddb python3-location
apt-get install python3-sphinx python3-sphinx-rtd-theme
apt-get install lsb-release dpkg-dev debhelper
```

### RHEL-based Linux

Some development packages may live in optional repositories. Check the files under `/etc/yum.repos.d/` and enable the
required repositories first. See [EPEL Quickstart](https://docs.fedoraproject.org/en-US/epel/#_quickstart).

```sh
# enable EPEL first

dnf install epel-release
dnf update

dnf install gcc pkgconf make capnproto
dnf install curl git jq tar xz
dnf install openssl-devel c-ares-devel lua-devel
dnf install python3-devel
dnf install python3-toml python3-requests python3-pycurl python3-semver
dnf install python3-maxminddb
dnf install python3-sphinx python3-sphinx_rtd_theme
dnf install rpmdevtools rpm-build
```

Some scripting or test dependencies may not be available in every repository set.

### macOS

```sh
brew install pkgconf capnp
brew install openssl c-ares
brew install lua
brew install python
```

The system toolchain from Xcode is also required.

### Windows

Native Windows builds are possible, but they usually require disabling some default features or using vendored
dependencies.

```powershell
# Rust MSVC toolchain
winget install Rustlang.Rust.MSVC

# Core tools
winget install capnproto.capnproto

# Option 1: vendored native libraries
winget install Kitware.CMake NASM.NASM Ninja-build.Ninja

# Option 2: vcpkg-managed libraries
vcpkg install --triplet=x64-windows-static-md openssl c-ares
$Env:VCPKG_ROOT = "C:\path\to\vcpkg"

# Example build without Python or Lua
cargo build --no-default-features --features rustls-ring,quic,c-ares
```

Tips:

- If `winget` is unavailable, install it from
  the [winget-cli releases](https://github.com/microsoft/winget-cli/releases).
- If you need a standalone `vcpkg` checkout:

```powershell
git clone https://github.com/microsoft/vcpkg.git
cd vcpkg
.\bootstrap-vcpkg.bat
```

Then add the install directory to `Path` and set `VCPKG_ROOT`.

### FreeBSD

```sh
pkg install rust
pkg install pkgconf capnproto
pkg install c-ares
# install a Lua package available on your release, for example lua54
pkg install lua54
pkg install python3
```

Tip: use the latest ports packages instead of quarterly packages if you need newer dependencies.

```sh
mkdir -p /usr/local/etc/pkg/repos/
echo 'FreeBSD: { url: "pkg+http://pkg.FreeBSD.org/${ABI}/latest" }' > /usr/local/etc/pkg/repos/FreeBSD.conf
pkg update -f
pkg upgrade -y
```

### NetBSD

```sh
pkgin install rust
pkgin install pkgconf capnproto
pkgin install libcares
# install a Lua package available on your system, for example lua54 or lua55
pkgin install lua55
# install Python and add a python3 symlink if needed
pkgin install python313
ln -s /usr/pkg/bin/python3.13 /usr/pkg/bin/python3
```

Tip: pkgsrc binary packages are available at <https://cdn.netbsd.org/pub/pkgsrc/packages/NetBSD/>. Update
`/usr/pkg/etc/pkgin/repositories.conf` if you need a newer package set.

### OpenBSD

```sh
pkg_add rust
pkg_add capnproto
pkg_add libcares
pkg_add lua
pkg_add python
```

Tip: if builds fail with an out-of-memory error, increase the `datasize-cur` limit for the `staff` login class in
`/etc/login.conf`.

### OmniOS

```sh
pkg install rust
pkg install pkg-config
# install capnproto and c-ares from source if packages are unavailable
```

## Dependency Reference

### Development libraries

`vey-proxy` can use:

```text
openssl >= 1.1.1
c-ares >= 1.18.0
lua >= 5.3 or LuaJIT
python3 >= 3.7
```

Lua support is disabled unless you explicitly enable a Lua feature such as `lua54`, `lua55`, or `luajit`.

### Development tools

Required for C or mixed-language builds:

```text
gcc or clang
pkg-config / pkgconf
```

Also required when the system `c-ares` is too old and you need a vendored build:

```text
cmake
```

### RPC code generation

The workspace uses Cap'n Proto RPC to communicate with running daemon processes:

```text
capnproto
```

### Test tools

Some test scripts require:

```text
curl
```

### Script tools

Scripts under `scripts/` commonly use:

```text
git
jq
tar
xz
```

### Python libraries for scripts

Some helper scripts require these Python packages:

```text
toml
requests
semver
PySocks
dnspython
maxminddb
```

### Documentation tools

The documentation toolchain uses [Sphinx](https://www.sphinx-doc.org/en/master/)
and [sphinx-rtd-theme](https://pypi.org/project/sphinx-rtd-theme/).

### Packaging tools

For Debian-based packaging:

```text
lsb-release
dpkg-dev
debhelper
```

For RHEL-based packaging:

```text
rpmdevtools
rpm-build
```
