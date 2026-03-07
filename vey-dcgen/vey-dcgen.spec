
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-dcgen
Version:        0.8.4
Release:        1%{?dist}
Summary:        Dynamic Certificate Generator

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, make, pkgconf
BuildRequires:  openssl-devel

%description
Dynamic Certificate Generator developed by the VEY project


%prep
%autosetup


%build
VEY_PACKAGE_VERSION="%{version}-%{release}"
export VEY_PACKAGE_VERSION
cargo build --frozen --offline --profile %{build_profile} --no-default-features --package vey-dcgen


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-dcgen %{buildroot}%{_bindir}/vey-dcgen
install -m 644 -D %{name}/debian/vey-dcgen@.service %{buildroot}/lib/systemd/system/vey-dcgen@.service


%files
%{_bindir}/vey-dcgen
/lib/systemd/system/vey-dcgen@.service
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN


%changelog
* Sat Aug 09 2025 VEY-OSS Developers <developers@vey.oss> - 0.8.4-1
- New upstream release
