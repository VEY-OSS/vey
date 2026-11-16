
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-proxy
Version:        1.15.2
Release:        1%{?dist}
Summary:        Modular Rust proxy server with built-in security auditing

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, gcc-c++, make, pkgconf, cmake
BuildRequires:  c-ares-devel, openssl-devel, libbpf-devel
BuildRequires:  python3-devel, lua-devel
BuildRequires:  systemd-rpm-macros
Recommends:     ca-certificates
%{?systemd_ordering}

%description
Modular async Rust proxy daemon supporting HTTP(S) and SOCKS4/5 forward
proxying, TCP/UDP TPROXY and SNI transparent proxying, TCP/UDP/TLS stream
proxying, and HTTP/TLS reverse proxying through site groups. Includes built-in
security auditing (TLS/TLCP interception, DPI, ICAP adaptation, and decrypted
traffic export), dynamic egress routing, proxy chaining, authentication, ACLs,
and hot reloads.


%prep
%autosetup


%build
VEY_PACKAGE_VERSION="%{version}-%{release}"
export VEY_PACKAGE_VERSION
%{?vey_pgo_profile:export VEY_PGO_PROFILE="%{vey_pgo_profile}"}
EBPF_FEATURE=$(sh scripts/package/detect_ebpf_feature.sh)
LUA_FEATURE=$(lua -v | sed 's/Lua \([0-9]\+\)[.]\([0-9]\+\)[.].*/lua\1\2/')
CARES_FEATURE=$(sh scripts/package/detect_c-ares_feature.sh)
scripts/package/cargo_with_pgo.sh build --frozen --profile %{build_profile} --no-default-features --features secure-snmalloc,$EBPF_FEATURE,$LUA_FEATURE,python,rustls-ring,quic,$CARES_FEATURE --package vey-proxy --package vey-proxy-ctl --package vey-proxy-lua --package vey-proxy-ftp


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-proxy %{buildroot}%{_bindir}/vey-proxy
install -m 755 -D target/%{build_profile}/vey-proxy-ctl %{buildroot}%{_bindir}/vey-proxy-ctl
install -m 755 -D target/%{build_profile}/vey-proxy-ftp %{buildroot}%{_bindir}/vey-proxy-ftp
install -m 755 -D target/%{build_profile}/vey-proxy-lua %{buildroot}%{_bindir}/vey-proxy-lua
install -m 644 -D %{name}/debian/vey-proxy@.service %{buildroot}%{_unitdir}/vey-proxy@.service


%post
if [ -d /run/systemd/system ]; then
	systemctl daemon-reload >/dev/null 2>&1 || :
fi

%postun
if [ -d /run/systemd/system ]; then
	systemctl daemon-reload >/dev/null 2>&1 || :
fi


%files
%{_bindir}/vey-proxy
%{_bindir}/vey-proxy-ctl
%{_bindir}/vey-proxy-ftp
%{_bindir}/vey-proxy-lua
%{_unitdir}/vey-proxy@.service
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN

%changelog
* Thu Nov 05 2026 VEY-OSS Developers <developers@vey.oss> - 1.15.2-1
- New upstream release
