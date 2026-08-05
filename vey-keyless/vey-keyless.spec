
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-keyless
Version:        0.5.0
Release:        1%{?dist}
Summary:        Keyless Server

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, gcc-c++, make, pkgconf, cmake
BuildRequires:  openssl-devel

%description
Keyless Server


%prep
%autosetup


%build
VEY_PACKAGE_VERSION="%{version}-%{release}"
export VEY_PACKAGE_VERSION
cargo build --frozen --offline --profile %{build_profile} --no-default-features --features secure-snmalloc,openssl-async-job --package vey-keyless --package vey-keyless-ctl


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-keyless %{buildroot}%{_bindir}/vey-keyless
install -m 755 -D target/%{build_profile}/vey-keyless-ctl %{buildroot}%{_bindir}/vey-keyless-ctl
install -m 644 -D %{name}/debian/vey-keyless@.service %{buildroot}/lib/systemd/system/vey-keyless@.service


%files
%{_bindir}/vey-keyless
%{_bindir}/vey-keyless-ctl
/lib/systemd/system/vey-keyless@.service
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN

%changelog
* Sun Jul 26 2026 VEY-OSS Developers <developers@vey.oss> - 0.5.0-1
- New upstream release
