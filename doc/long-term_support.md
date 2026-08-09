Long-Term Support
-----

# Policy

We will offer LTS branches for applications that is considered to be production ready,
the branch name will be in the format **lts/\<name\>/\<version\>/\<feature\>**, such as *lts/vey-proxy/1.14/default*.

LTS branches will only get bug & security fixes, so there won't be any new features or breaking changes.
The dependency lock file `Cargo.lock` will only get semver compatible updates when necessary.

Each LTS branch will be supported 6 months after the next LTS branch for the same application.
You can ask for commercial support if you need a longer support time.

Release notes for each LTS cut live under [doc/lts/](lts/).

# Next LTS branches

# Current LTS branches

## [vey-proxy-v1.14](https://github.com/VEY-OSS/vey/tree/lts/vey-proxy/1.14/default)

Long-Term branch for

- [vey-proxy](../vey-proxy) 1.14.x
- [vey-dcgen](../vey-dcgen) 0.9.x
- [vey-iploc](../vey-iploc) 0.4.x

Release notes for the first 1.14.0 cut:

- [English](lts/vey-proxy-v1.14.0.md)
- [简体中文](lts/vey-proxy-v1.14.0.zh_CN.md)

Minimum requirements:

- MSRV: 1.94
- Linux OS: Debian 12 and RHEL 9 (see [Target Platforms](target_platforms.md)).

## [g3proxy-v1.12](https://github.com/bytedance/g3/tree/lts/g3proxy/1.12/default)

Long-Term branch for

- [g3proxy](https://github.com/bytedance/g3/tree/lts/g3proxy/1.12/default/g3proxy) 1.12.x
- [g3fcgen](https://github.com/bytedance/g3/tree/lts/g3proxy/1.12/default/g3fcgen) 0.8.x
- [g3iploc](https://github.com/bytedance/g3/tree/lts/g3proxy/1.12/default/g3iploc) 0.3.x

This line remains supported for **6 months** after the vey-proxy 1.14 LTS branch
starts. Prefer migrating to [vey-proxy 1.14](#vey-proxy-v114) when possible; see
[Migration from G3 to VEY](migrate_from_g3_to_vey.md).

Minimum requirements:

- MSRV: 1.86
- Linux OS: Debian 11 and CentOS 8.
