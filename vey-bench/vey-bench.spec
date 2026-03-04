
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-bench
Version:        0.9.7
Release:        1%{?dist}
Summary:        Multi-Target Benchmark tool developed by the VEY project

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, make, pkgconf
BuildRequires:  openssl-devel

Requires:       ca-certificates

%description
Multi-Target Benchmark tool developed by the VEY project


%prep
%autosetup


%build
G3_PACKAGE_VERSION="%{version}-%{release}"
export G3_PACKAGE_VERSION
cargo build --frozen --offline --profile %{build_profile} --no-default-features --features rustls-ring,quic --package vey-bench


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-bench %{buildroot}%{_bindir}/vey-bench


%files
%{_bindir}/vey-bench
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN


%changelog
* Thu Jan 15 2026 VEY-OSS Developers <developers@vey.oss> - 0.9.7-1
- New upstream release
