
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-iploc
Version:        0.4.0
Release:        1%{?dist}
Summary:        IP Locate Service

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, gcc-c++, make, pkgconf, cmake

%description
IP Locate Service


%prep
%autosetup


%build
VEY_PACKAGE_VERSION="%{version}-%{release}"
export VEY_PACKAGE_VERSION
cargo build --frozen --offline --profile %{build_profile} --features secure-snmalloc --package vey-iploc


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-iploc %{buildroot}%{_bindir}/vey-iploc
install -m 644 -D %{name}/debian/vey-iploc@.service %{buildroot}/lib/systemd/system/vey-iploc@.service


%files
%{_bindir}/vey-iploc
/lib/systemd/system/vey-iploc@.service
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN


%changelog
* Sun Jul 26 2026 VEY-OSS Developers <developers@vey.oss> - 0.4.0-1
- New upstream release
