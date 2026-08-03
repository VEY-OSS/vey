/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::io::Write;
#[cfg(unix)]
use std::path::PathBuf;
use std::time::Duration;

use anyhow::anyhow;
use http::HeaderName;
use rustls_pki_types::ServerName;
use url::Url;

use vey_types::net::{
    ConnectionPoolConfig, Host, HttpAuth, RustlsClientConfigBuilder, TcpKeepAliveConfig,
    UpstreamAddr,
};

#[cfg(feature = "yaml")]
mod yaml;

use super::IcapMethod;

pub struct IcapServiceConfig {
    pub(crate) method: IcapMethod,
    url: Url,
    auth: HttpAuth,
    user_agent: Option<String>,
    pub(crate) upstream: UpstreamAddr,
    pub(crate) tls_client: Option<RustlsClientConfigBuilder>,
    pub(crate) tls_name: ServerName<'static>,
    pub(crate) connection_pool: ConnectionPoolConfig,
    pub(crate) tcp_keepalive: TcpKeepAliveConfig,
    pub(crate) tcp_connect_timeout: Duration,
    #[cfg(unix)]
    pub(crate) use_unix_socket: Option<PathBuf>,
    pub(crate) icap_206_enable: bool,
    pub(crate) icap_max_header_size: usize,
    pub(crate) disable_preview: bool,
    pub(crate) preview_data_read_timeout: Duration,
    pub(crate) respond_shared_names: BTreeSet<String>,
    pub(crate) bypass: bool,
}

impl IcapServiceConfig {
    pub fn new(method: IcapMethod, mut url: Url) -> anyhow::Result<Self> {
        let (tls_client, default_port) = match url.scheme().to_ascii_lowercase().as_str() {
            "icap" => (None, 1344u16),
            "icaps" => (Some(RustlsClientConfigBuilder::default()), 11344u16),
            _ => return Err(anyhow!("unsupported ICAP URL scheme: {}", url.scheme())),
        };

        if !url.has_authority() {
            return Err(anyhow!("no authority part found in this url"));
        }
        let auth = HttpAuth::try_from(&url).map_err(|e| anyhow!("invalid auth info: {e}"))?;
        url.set_username("")
            .map_err(|_| anyhow!("failed to clear username in url"))?;
        url.set_password(None)
            .map_err(|_| anyhow!("failed to clear password in url"))?;

        let host = url
            .host()
            .ok_or_else(|| anyhow!("no host found in this url"))?;
        let host = Host::try_from(host)
            .map_err(|e| anyhow!("failed to get upstream address from url: {e}"))?;
        let upstream = UpstreamAddr::new(host, url.port().unwrap_or(default_port));
        let tls_name = ServerName::try_from(upstream.host())
            .map_err(|e| anyhow!("invalid ICAP server name: {e}"))?;
        Ok(IcapServiceConfig {
            method,
            url,
            auth,
            user_agent: None,
            upstream,
            tls_client,
            tls_name,
            connection_pool: ConnectionPoolConfig::default(),
            tcp_keepalive: TcpKeepAliveConfig::default_enabled(),
            tcp_connect_timeout: Duration::from_secs(1),
            #[cfg(unix)]
            use_unix_socket: None,
            icap_206_enable: false,
            icap_max_header_size: 8192,
            disable_preview: false,
            preview_data_read_timeout: Duration::from_secs(4),
            respond_shared_names: BTreeSet::new(),
            bypass: false,
        })
    }

    pub fn set_tcp_keepalive(&mut self, config: TcpKeepAliveConfig) {
        self.tcp_keepalive = config;
    }

    pub fn set_tcp_connect_timeout(&mut self, time: Duration) {
        self.tcp_connect_timeout = time;
    }

    pub fn set_tls_client(&mut self, config: RustlsClientConfigBuilder) {
        self.tls_client = Some(config);
    }

    pub fn set_tls_name(&mut self, name: ServerName<'static>) {
        self.tls_name = name;
    }

    pub fn set_icap_max_header_size(&mut self, max_size: usize) {
        self.icap_max_header_size = max_size;
    }

    pub fn set_preview_data_read_timeout(&mut self, time: Duration) {
        self.preview_data_read_timeout = time;
    }

    pub fn set_bypass(&mut self, bypass: bool) {
        self.bypass = bypass;
    }

    pub fn add_respond_shared_name(&mut self, name: HeaderName) {
        self.respond_shared_names.insert(name.as_str().to_owned());
    }

    pub(crate) fn build_request_header(&self) -> Vec<u8> {
        let mut header = Vec::with_capacity(1024);
        self.write_header(&mut header, self.method.as_str());
        header
    }

    pub(crate) fn build_options_request(&self) -> Vec<u8> {
        let mut header = Vec::with_capacity(256);
        self.write_header(&mut header, "OPTIONS");
        header
    }

    fn write_header(&self, header: &mut Vec<u8>, method: &str) {
        let _ = write!(header, "{method} {} ICAP/1.0\r\n", self.url);
        if let Some(host) = self.url.host() {
            self.write_host_header(header, host);
        }
        if let Some(user_agent) = &self.user_agent {
            let _ = write!(header, "User-Agent: {user_agent}\r\n");
        }
        match &self.auth {
            HttpAuth::None => {}
            HttpAuth::Basic(basic_auth) => {
                let _ = write!(
                    header,
                    "Authorization: Basic {}\r\n",
                    basic_auth.encoded_value()
                );
            }
        }
    }

    fn write_host_header(&self, header: &mut Vec<u8>, host: url::Host<&str>) {
        let default_port = match self.url.scheme().to_ascii_lowercase().as_str() {
            "icaps" => 11344,
            _ => 1344,
        };
        let include_port = self.url.port().is_some_and(|port| port != default_port);

        match host {
            url::Host::Domain(domain) => {
                if include_port {
                    let _ = write!(header, "Host: {domain}:{}\r\n", self.url.port().unwrap());
                } else {
                    let _ = write!(header, "Host: {domain}\r\n");
                }
            }
            url::Host::Ipv4(ip) => {
                if include_port {
                    let _ = write!(header, "Host: {ip}:{}\r\n", self.url.port().unwrap());
                } else {
                    let _ = write!(header, "Host: {ip}\r\n");
                }
            }
            url::Host::Ipv6(ip) => {
                if include_port {
                    let _ = write!(header, "Host: [{ip}]:{}\r\n", self.url.port().unwrap());
                } else {
                    let _ = write!(header, "Host: [{ip}]\r\n");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_unsupported_scheme() {
        let url = Url::parse("http://icap.example/reqmod").unwrap();
        match IcapServiceConfig::new(IcapMethod::Reqmod, url) {
            Err(err) => assert!(err.to_string().contains("unsupported ICAP URL scheme")),
            Ok(_) => panic!("expected unsupported scheme error"),
        }
    }

    #[test]
    fn new_accepts_icap_and_builds_request_header() {
        let url = Url::parse("icap://icap.example:1344/reqmod").unwrap();
        let mut config = IcapServiceConfig::new(IcapMethod::Reqmod, url).unwrap();
        assert!(config.tls_client.is_none());
        assert_eq!(config.upstream.port(), 1344);
        assert_eq!(config.tcp_connect_timeout, Duration::from_secs(1));
        config.user_agent = Some("vey-test/1.0".to_string());

        let header = String::from_utf8(config.build_request_header()).unwrap();
        assert!(header.starts_with("REQMOD icap://icap.example:1344/reqmod ICAP/1.0\r\n"));
        assert!(header.contains("Host: icap.example\r\n"));
        assert!(header.contains("User-Agent: vey-test/1.0\r\n"));

        let options = String::from_utf8(config.build_options_request()).unwrap();
        assert!(options.starts_with("OPTIONS icap://icap.example:1344/reqmod ICAP/1.0\r\n"));
    }

    #[test]
    fn new_uses_default_icap_port() {
        let url = Url::parse("icap://icap.example/reqmod").unwrap();
        let config = IcapServiceConfig::new(IcapMethod::Reqmod, url).unwrap();
        assert_eq!(config.upstream.port(), 1344);
        assert_eq!(config.url.to_string(), "icap://icap.example/reqmod");
    }

    #[test]
    fn new_accepts_icaps_with_default_port() {
        let url = Url::parse("icaps://secure.example/respmod").unwrap();
        let config = IcapServiceConfig::new(IcapMethod::Respmod, url).unwrap();
        assert!(config.tls_client.is_some());
        assert_eq!(config.upstream.port(), 11344);
    }

    #[test]
    fn new_with_basic_auth_adds_authorization() {
        let url = Url::parse("icap://user:secret@icap.example/reqmod").unwrap();
        let config = IcapServiceConfig::new(IcapMethod::Reqmod, url).unwrap();
        assert_eq!(config.upstream.port(), 1344);
        let header = String::from_utf8(config.build_request_header()).unwrap();
        assert!(header.contains("Authorization: Basic "));
        assert!(!header.contains("user:secret@"));
    }

    #[test]
    fn host_header_includes_non_default_port() {
        let url = Url::parse("icap://icap.example:2000/reqmod").unwrap();
        let config = IcapServiceConfig::new(IcapMethod::Reqmod, url).unwrap();
        let header = String::from_utf8(config.build_request_header()).unwrap();
        assert!(header.contains("Host: icap.example:2000\r\n"));
    }

    #[test]
    fn host_header_omits_default_icaps_port() {
        let url = Url::parse("icaps://secure.example:11344/respmod").unwrap();
        let config = IcapServiceConfig::new(IcapMethod::Respmod, url).unwrap();
        let header = String::from_utf8(config.build_request_header()).unwrap();
        assert!(header.contains("Host: secure.example\r\n"));
        assert!(!header.contains("Host: secure.example:11344\r\n"));
    }

    #[test]
    fn host_header_brackets_ipv6_with_non_default_port() {
        let url = Url::parse("icap://[2001:db8::1]:2000/reqmod").unwrap();
        let config = IcapServiceConfig::new(IcapMethod::Reqmod, url).unwrap();
        let header = String::from_utf8(config.build_request_header()).unwrap();
        assert!(header.contains("Host: [2001:db8::1]:2000\r\n"));
    }
}
