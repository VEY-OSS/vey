
%undefine _debugsource_packages
%define build_profile release-lto

Name:           vey-iploc
Version:        0.5.0
Release:        1%{?dist}
Summary:        IP geolocation lookup service

License:        Apache-2.0
URL:            https://github.com/VEY-OSS/vey
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  gcc, gcc-c++, make, pkgconf, cmake, capnproto, clang, libbpf-devel
BuildRequires:  systemd-rpm-macros
%{?systemd_ordering}

%description
Serves IP-to-country and ASN lookups used by vey-proxy GeoIP routing.


%prep
%autosetup


%build
VEY_PACKAGE_VERSION="%{version}-%{release}"
export VEY_PACKAGE_VERSION
%{?vey_pgo_profile:export VEY_PGO_PROFILE="%{vey_pgo_profile}"}
EBPF_FEATURE=$(sh scripts/package/detect_ebpf_feature.sh)
scripts/package/cargo_with_pgo.sh build --frozen --offline --profile %{build_profile} --features secure-snmalloc,$EBPF_FEATURE --package vey-iploc --package vey-iploc-ctl


%install
rm -rf $RPM_BUILD_ROOT
install -m 755 -D target/%{build_profile}/vey-iploc %{buildroot}%{_bindir}/vey-iploc
install -m 755 -D target/%{build_profile}/vey-iploc-ctl %{buildroot}%{_bindir}/vey-iploc-ctl
install -m 755 -D scripts/geoip-dump/geoip_dump_country.py %{buildroot}%{_datadir}/vey-iploc/geoip_dump_country.py
install -m 755 -D scripts/geoip-dump/geoip_dump_asn.py %{buildroot}%{_datadir}/vey-iploc/geoip_dump_asn.py
install -m 644 -D %{name}/debian/vey-iploc@.service %{buildroot}%{_unitdir}/vey-iploc@.service


%post
if [ -d /run/systemd/system ]; then
	systemctl daemon-reload >/dev/null 2>&1 || :
fi

%postun
if [ -d /run/systemd/system ]; then
	systemctl daemon-reload >/dev/null 2>&1 || :
fi


%files
%{_bindir}/vey-iploc
%{_bindir}/vey-iploc-ctl
%dir %{_datadir}/vey-iploc
%{_datadir}/vey-iploc/geoip_dump_country.py
%{_datadir}/vey-iploc/geoip_dump_asn.py
%{_unitdir}/vey-iploc@.service
%license LICENSE
%license LICENSE-BUNDLED
%license LICENSE-FOREIGN


%changelog
* Fri Oct 30 2026 VEY-OSS Developers <developers@vey.oss> - 0.5.0-1
- New upstream release
