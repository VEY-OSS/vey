
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-gateway
Version:        0.4.0
Release:        1%{?dist}
Summary:        Programmable reverse proxy and gateway

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, gcc-c++, make, pkgconf, cmake
BuildRequires:  openssl-devel, libbpf-devel
BuildRequires:  systemd-rpm-macros
Recommends:     ca-certificates
%{?systemd_ordering}

%description
A reverse-proxy and gateway daemon for multiple frontend and upstream
protocols. The current implementation focuses on TLS and keyless traffic
handling.


%prep
%autosetup


%build
VEY_PACKAGE_VERSION="%{version}-%{release}"
export VEY_PACKAGE_VERSION
%{?vey_pgo_profile:export VEY_PGO_PROFILE="%{vey_pgo_profile}"}
EBPF_FEATURE=$(sh scripts/package/detect_ebpf_feature.sh)
scripts/package/cargo_with_pgo.sh build --frozen --offline --profile %{build_profile} --no-default-features --features secure-snmalloc,$EBPF_FEATURE,rustls-ring,quic --package vey-gateway --package vey-gateway-ctl


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-gateway %{buildroot}%{_bindir}/vey-gateway
install -m 755 -D target/%{build_profile}/vey-gateway-ctl %{buildroot}%{_bindir}/vey-gateway-ctl
install -m 644 -D %{name}/debian/vey-gateway@.service %{buildroot}%{_unitdir}/vey-gateway@.service


%post
if [ -d /run/systemd/system ]; then
	systemctl daemon-reload >/dev/null 2>&1 || :
fi

%postun
if [ -d /run/systemd/system ]; then
	systemctl daemon-reload >/dev/null 2>&1 || :
fi


%files
%{_bindir}/vey-gateway
%{_bindir}/vey-gateway-ctl
%{_unitdir}/vey-gateway@.service
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN

%changelog
* Sun Jul 26 2026 VEY-OSS Developers <developers@vey.oss> - 0.4.0-1
- New upstream release
