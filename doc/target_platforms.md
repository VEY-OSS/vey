# Target Platforms

Linux is the primary fully supported platform family, with distro and version targets defined below.

VEY also targets the following non-Linux platforms, but current testing coverage is limited and contributions to improve
validation are welcome:

- macOS
- Windows >= 10
- FreeBSD >= 14.3
- NetBSD >= 10.1
- OpenBSD >= 7.8

## Linux Distribution Support Policy

The following Linux distributions are fully supported:

- RHEL: 10, 9
- Debian: 13, 12
- Ubuntu: 26.04 LTS, 24.04 LTS
- openSUSE Leap: 16.0

Other Linux distributions with newer dependencies should also work. Old versions may work with some features disabled.

## Supported Architectures

The following architectures are VEY's primary architecture support targets. They are simplified user-facing names for
the architectures currently covered by the cross-build GitHub workflow in [`cross.yml`](../.github/workflows/cross.yml).
These architecture targets are primarily validated through Linux cross-build CI. Other architectures may also work, but
they are not part of the regular test matrix.

Feature availability may still vary by operating system, libc environment,
system libraries, and kernel capabilities, especially for networking and TLS
related functionality.

- x86-64: mainstream 64-bit x86 systems, including most modern servers, desktops, and virtual machines.
- x86 (32-bit): legacy 32-bit x86 systems.
- Arm64: 64-bit ARM systems, including many cloud instances, servers, and newer embedded platforms.
- Armv7: 32-bit ARMv7 systems, commonly used on older embedded and edge devices.
- Arm: older 32-bit ARM systems outside the Armv7 target level.
- RISC-V 64-bit: 64-bit RISC-V systems.
- POWER little-endian (64-bit): 64-bit little-endian POWER systems, such as modern POWER Linux deployments.
- IBM Z: IBM mainframe systems using the s390x architecture.
- LoongArch64: 64-bit LoongArch systems.
