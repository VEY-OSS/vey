
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-statsd
Version:        0.1.0
Release:        1%{?dist}
Summary:        StatsD Server

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, make, pkgconf
BuildRequires:  openssl-devel,

%description
StatsD Server


%prep
%autosetup


%build
VEY_PACKAGE_VERSION="%{version}-%{release}"
export VEY_PACKAGE_VERSION
cargo build --frozen --offline --profile %{build_profile} --package vey-statsd --package vey-statsd-ctl


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-statsd %{buildroot}%{_bindir}/vey-statsd
install -m 755 -D target/%{build_profile}/vey-statsd-ctl %{buildroot}%{_bindir}/vey-statsd-ctl
install -m 644 -D %{name}/debian/vey-statsd@.service %{buildroot}/lib/systemd/system/vey-statsd@.service


%files
%{_bindir}/vey-statsd
%{_bindir}/vey-statsd-ctl
/lib/systemd/system/vey-statsd@.service
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN
%doc sphinx/%{name}/_build/html


%changelog
* Tue May 13 2025 VEY-OSS Developers <developers@vey.oss> - 0.1.0-1
- New upstream release
