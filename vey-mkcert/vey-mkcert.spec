
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-mkcert
Version:        0.1.0
Release:        1%{?dist}
Summary:        Tool to make certificates

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, make, pkgconf
BuildRequires:  openssl-devel

%description
Tool to make certificates


%prep
%autosetup


%build
G3_PACKAGE_VERSION="%{version}-%{release}"
export G3_PACKAGE_VERSION
cargo build --frozen --offline --profile %{build_profile} --no-default-features --package vey-mkcert


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-mkcert %{buildroot}%{_bindir}/vey-mkcert


%files
%{_bindir}/vey-mkcert
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN


%changelog
* Thu May 04 2023 VEY-OSS Developers <developers@vey.oss> - 0.1.0-1
- New upstream release
