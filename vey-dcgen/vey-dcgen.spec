
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-dcgen
Version:        0.9.0
Release:        1%{?dist}
Summary:        Dynamic certificate generator for TLS interception

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, gcc-c++, make, pkgconf, cmake
BuildRequires:  openssl-devel
BuildRequires:  systemd-rpm-macros
%{?systemd_ordering}

%description
Issues per-host certificates for vey-proxy TLS interception from a configured
certificate authority.


%prep
%autosetup


%build
VEY_PACKAGE_VERSION="%{version}-%{release}"
export VEY_PACKAGE_VERSION
%{?vey_pgo_profile:export VEY_PGO_PROFILE="%{vey_pgo_profile}"}
scripts/package/cargo_with_pgo.sh build --frozen --offline --profile %{build_profile} --no-default-features --features secure-snmalloc --package vey-dcgen


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-dcgen %{buildroot}%{_bindir}/vey-dcgen
install -m 644 -D %{name}/debian/vey-dcgen@.service %{buildroot}%{_unitdir}/vey-dcgen@.service


%post
if [ -d /run/systemd/system ]; then
	systemctl daemon-reload >/dev/null 2>&1 || :
fi

%postun
if [ -d /run/systemd/system ]; then
	systemctl daemon-reload >/dev/null 2>&1 || :
fi


%files
%{_bindir}/vey-dcgen
%{_unitdir}/vey-dcgen@.service
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN


%changelog
* Sun Jul 26 2026 VEY-OSS Developers <developers@vey.oss> - 0.9.0-1
- New upstream release
