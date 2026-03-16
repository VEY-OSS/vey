
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-proxy
Version:        1.13.0
Release:        1%{?dist}
Summary:        Generic Proxy Server

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, make, pkgconf, cmake
BuildRequires:  lua-devel, openssl-devel
Requires:       ca-certificates

%description
Generic Proxy Server


%prep
%autosetup


%build
VEY_PACKAGE_VERSION="%{version}-%{release}"
export VEY_PACKAGE_VERSION
LUA_FEATURE=$(lua -v | sed 's/Lua \([0-9]\+\)[.]\([0-9]\+\)[.].*/lua\1\2/')
CARES_FEATURE=$(sh scripts/package/detect_c-ares_feature.sh)
cargo build --frozen --profile %{build_profile} --no-default-features --features $LUA_FEATURE,rustls-ring,quic,$CARES_FEATURE --package vey-proxy --package vey-proxy-ctl --package vey-proxy-lua --package vey-proxy-ftp


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-proxy %{buildroot}%{_bindir}/vey-proxy
install -m 755 -D target/%{build_profile}/vey-proxy-ctl %{buildroot}%{_bindir}/vey-proxy-ctl
install -m 755 -D target/%{build_profile}/vey-proxy-ftp %{buildroot}%{_bindir}/vey-proxy-ftp
install -m 755 -D target/%{build_profile}/vey-proxy-lua %{buildroot}%{_bindir}/vey-proxy-lua
install -m 644 -D %{name}/debian/vey-proxy@.service %{buildroot}/lib/systemd/system/vey-proxy@.service


%files
%{_bindir}/vey-proxy
%{_bindir}/vey-proxy-ctl
%{_bindir}/vey-proxy-ftp
%{_bindir}/vey-proxy-lua
/lib/systemd/system/vey-proxy@.service
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN
%doc sphinx/%{name}/_build/html


%changelog
* Sun Mar 15 2026 VEY-OSS Developers <developers@vey.oss> - 1.13.0-1
- New upstream release
