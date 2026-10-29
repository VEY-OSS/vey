
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-statsd
Version:        0.3.0
Release:        1%{?dist}
Summary:        StatsD metrics ingestion and export server

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, gcc-c++, make, pkgconf, cmake
BuildRequires:  systemd-rpm-macros
%{?systemd_ordering}

%description
Receives StatsD metrics over UDP or a Unix datagram socket, aggregates them,
and exports to backends such as Graphite, Prometheus, InfluxDB, and
VictoriaMetrics.


%prep
%autosetup


%build
VEY_PACKAGE_VERSION="%{version}-%{release}"
export VEY_PACKAGE_VERSION
%{?vey_pgo_profile:export VEY_PGO_PROFILE="%{vey_pgo_profile}"}
scripts/package/cargo_with_pgo.sh build --frozen --offline --profile %{build_profile} --features secure-snmalloc --package vey-statsd --package vey-statsd-ctl


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-statsd %{buildroot}%{_bindir}/vey-statsd
install -m 755 -D target/%{build_profile}/vey-statsd-ctl %{buildroot}%{_bindir}/vey-statsd-ctl
install -m 644 -D %{name}/debian/vey-statsd@.service %{buildroot}%{_unitdir}/vey-statsd@.service


%post
if [ -d /run/systemd/system ]; then
	systemctl daemon-reload >/dev/null 2>&1 || :
fi

%postun
if [ -d /run/systemd/system ]; then
	systemctl daemon-reload >/dev/null 2>&1 || :
fi


%files
%{_bindir}/vey-statsd
%{_bindir}/vey-statsd-ctl
%{_unitdir}/vey-statsd@.service
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN

%changelog
* Sun Oct 11 2026 VEY-OSS Developers <developers@vey.oss> - 0.3.0-1
- New upstream release
