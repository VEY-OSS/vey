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

For Linux distributions, the following policies and version lists define VEY's primary support targets. The code is
still expected to work on other distributions and may also work on older versions, but those environments are not
primary support targets.

For the primary target Linux distributions listed below, VEY also provides prebuilt binary packages
through [cloudsmith](https://cloudsmith.io/~vey-oss/repos/).

- RHEL: only versions that are in Red Hat's Full Support phase are supported.
- Debian: only versions that still receive security updates from the Debian Security Team are supported.
- Ubuntu: only LTS releases that are still in Canonical's Standard Security Maintenance window are supported.
- SUSE Enterprise: only currently supported openSUSE Leap releases are supported.

Current supported versions as of 2026-04-27:

This table is date-sensitive and should be updated when upstream vendor support windows change.

| Distribution family | Support policy                              | Currently supported versions    |
|---------------------|---------------------------------------------|---------------------------------|
| RHEL                | Full Support only                           | 10, 9                           |
| Debian              | Debian Security Team support only           | 13 (stable), 12 (oldstable)     |
| Ubuntu              | LTS with Standard Security Maintenance only | 26.04 LTS, 24.04 LTS, 22.04 LTS |
| openSUSE Leap       | Supported Leap releases only                | 16.0                            |

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
