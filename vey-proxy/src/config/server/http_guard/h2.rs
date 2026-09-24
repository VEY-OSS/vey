/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HttpGuardH2Config {
    pub(crate) max_header_list_size: u32,
    pub(crate) max_concurrent_streams: u32,
    stream_window_size: u32,
    connection_window_size: u32,
    max_frame_size: u32,
    pub(crate) max_send_buffer_size: usize,
    pub(crate) upstream_handshake_timeout: Duration,
    pub(crate) upstream_stream_open_timeout: Duration,
    pub(crate) client_handshake_timeout: Duration,
}

impl Default for HttpGuardH2Config {
    fn default() -> Self {
        HttpGuardH2Config {
            max_header_list_size: 64 * 1024,
            max_concurrent_streams: 128,
            stream_window_size: 1024 * 1024,
            connection_window_size: 2 * 1024 * 1024,
            max_frame_size: 256 * 1024,
            max_send_buffer_size: 8 * 1024 * 1024,
            upstream_handshake_timeout: Duration::from_secs(10),
            upstream_stream_open_timeout: Duration::from_secs(10),
            client_handshake_timeout: Duration::from_secs(4),
        }
    }
}

impl HttpGuardH2Config {
    pub(crate) fn build_server(&self) -> h2::server::Builder {
        let mut builder = h2::server::Builder::new();
        builder
            .max_header_list_size(self.max_header_list_size)
            .max_concurrent_streams(self.max_concurrent_streams)
            .max_frame_size(self.max_frame_size)
            .max_send_buffer_size(self.max_send_buffer_size)
            .initial_window_size(self.stream_window_size)
            .initial_connection_window_size(self.connection_window_size)
            .enable_connect_protocol();
        builder
    }

    pub(crate) fn build_client(&self) -> h2::client::Builder {
        let mut builder = h2::client::Builder::new();
        builder
            .enable_push(false)
            .max_header_list_size(self.max_header_list_size)
            .max_concurrent_streams(0)
            .max_frame_size(self.max_frame_size)
            .max_send_buffer_size(self.max_send_buffer_size)
            .initial_window_size(self.stream_window_size)
            .initial_connection_window_size(self.connection_window_size);
        builder
    }

    pub(super) fn parse_yaml(&mut self, value: &Yaml) -> anyhow::Result<()> {
        let Yaml::Hash(map) = value else {
            return Err(anyhow!("yaml value type for 'h2' should be 'map'"));
        };
        vey_yaml::foreach_kv(map, |k, v| self.set(k, v))
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "max_header_list_size" | "max_header_size" => {
                self.max_header_list_size = vey_yaml::humanize::as_u32(v)
                    .context(format!("invalid humanize u32 value for key {k}"))?;
                Ok(())
            }
            "max_concurrent_streams" => {
                self.max_concurrent_streams = vey_yaml::value::as_u32(v)?;
                Ok(())
            }
            "max_frame_size" => {
                let size = vey_yaml::humanize::as_u32(v)
                    .context(format!("invalid humanize u32 value for key {k}"))?;
                self.max_frame_size = size.clamp(1 << 14, (1 << 24) - 1);
                Ok(())
            }
            "stream_window_size" => {
                let size = vey_yaml::humanize::as_u32(v)
                    .context(format!("invalid humanize u32 value for key {k}"))?;
                self.stream_window_size = size.max(65536);
                Ok(())
            }
            "connection_window_size" => {
                let size = vey_yaml::humanize::as_u32(v)
                    .context(format!("invalid humanize u32 value for key {k}"))?;
                self.connection_window_size = size.max(65536);
                Ok(())
            }
            "max_send_buffer_size" => {
                self.max_send_buffer_size = vey_yaml::humanize::as_usize(v)
                    .context(format!("invalid humanize usize value for key {k}"))?;
                Ok(())
            }
            "upstream_handshake_timeout" => {
                self.upstream_handshake_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "upstream_stream_open_timeout" => {
                self.upstream_stream_open_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "client_handshake_timeout" => {
                self.client_handshake_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}
