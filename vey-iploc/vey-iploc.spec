
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-iploc
Version:        0.3.0
Release:        1%{?dist}
Summary:        IP Locate Service

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

%description
IP Locate Service


%prep
%autosetup


%build
G3_PACKAGE_VERSION="%{version}-%{release}"
export G3_PACKAGE_VERSION
cargo build --frozen --offline --profile %{build_profile} --package vey-iploc


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
* Sat Aug 09 2025 VEY-OSS Developers <developers@vey.oss> - 0.3.0-1
- New upstream release
