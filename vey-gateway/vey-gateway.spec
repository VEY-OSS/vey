
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-gateway
Version:        0.3.9
Release:        1%{?dist}
Summary:        Generic Gateway

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, make, pkgconf
BuildRequires:  openssl-devel,
BuildRequires:  libtool
Requires:       ca-certificates

%description
Generic Gateway


%prep
%autosetup


%build
G3_PACKAGE_VERSION="%{version}-%{release}"
export G3_PACKAGE_VERSION
cargo build --frozen --offline --profile %{build_profile} --no-default-features --features rustls-ring,quic --package vey-gateway --package vey-gateway-ctl
sh %{name}/service/generate_systemd.sh


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-gateway %{buildroot}%{_bindir}/vey-gateway
install -m 755 -D target/%{build_profile}/vey-gateway-ctl %{buildroot}%{_bindir}/vey-gateway-ctl
install -m 644 -D %{name}/debian/vey-gateway@.service %{buildroot}/lib/systemd/system/vey-gateway@.service


%files
%{_bindir}/vey-gateway
%{_bindir}/vey-gateway-ctl
/lib/systemd/system/vey-gateway@.service
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN
%doc sphinx/%{name}/_build/html


%changelog
* Mon Jul 14 2025 VEY-OSS Developers <developers@vey.oss> - 0.3.9-1
- New upstream release
