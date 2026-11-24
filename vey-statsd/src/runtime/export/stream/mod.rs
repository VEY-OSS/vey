/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::time::Duration;

use log::{debug, warn};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::sync::mpsc;

use vey_io_ext::LimitedWriteExt;

mod config;
pub(crate) use config::StreamExportConfig;

const BATCH_SIZE: usize = 16;

pub(crate) trait StreamExport {
    type Piece;

    fn serialize(&self, record: &[Self::Piece], buf: &mut Vec<u8>) -> usize;
}

pub(crate) struct StreamExportRuntime<T: StreamExport> {
    config: StreamExportConfig,
    formatter: T,
    receiver: mpsc::UnboundedReceiver<T::Piece>,

    recv_buf: Vec<T::Piece>,
    recv_handled: usize,
    write_buf: Vec<u8>,
    quit: bool,
}

impl<T> StreamExportRuntime<T>
where
    T: StreamExport,
{
    pub(crate) fn new(
        config: StreamExportConfig,
        formatter: T,
        receiver: mpsc::UnboundedReceiver<T::Piece>,
    ) -> Self {
        StreamExportRuntime {
            config,
            formatter,
            receiver,
            recv_buf: Vec::with_capacity(BATCH_SIZE),
            recv_handled: 0,
            write_buf: Vec::with_capacity(2048),
            quit: false,
        }
    }

    pub(crate) async fn into_running(mut self) {
        loop {
            if self.recv_handled >= self.recv_buf.len() {
                self.recv_buf.clear();
                self.recv_handled = 0;
                let n = self
                    .receiver
                    .recv_many(&mut self.recv_buf, BATCH_SIZE)
                    .await;
                if n == 0 {
                    break;
                }
            }
            match self.config.connect().await {
                Ok(stream) => self.run_with_stream(stream).await,
                Err(wait) => self.drop_wait(wait).await,
            }
            if self.quit {
                break;
            }
        }
    }

    async fn drop_wait(&mut self, wait: Duration) {
        if tokio::time::timeout(wait, async {
            while self.receiver.recv().await.is_some() {
                // TODO add metrics
            }
        })
        .await
        .is_ok()
        {
            self.quit = true
        }
    }

    async fn run_with_stream<S>(&mut self, mut stream: S)
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let mut read_buf = [0u8; BATCH_SIZE];

        loop {
            if self.recv_handled < self.recv_buf.len() {
                if let Err(e) = self.send_records(&mut stream).await {
                    warn!(
                        "exporter {}: failed to send records: {e:?}",
                        self.config.exporter
                    );
                    break;
                }
                continue;
            } else {
                self.recv_buf.clear();
                self.recv_handled = 0;
            }

            tokio::select! {
                biased;

                r = stream.read(&mut read_buf) => {
                    match r {
                        Ok(0) => {
                            debug!("exporter {}: connection closed by peer", self.config.exporter);
                        }
                        Ok(_) => {
                            debug!("exporter {}: close connection as unexpected data received", self.config.exporter);
                        }
                        Err(e) => {
                            debug!("exporter {}: connection closed by peer: {e}", self.config.exporter);
                        }
                    }
                    break;
                }
                n = self.receiver.recv_many(&mut self.recv_buf, BATCH_SIZE) => {
                    if n == 0 {
                        self.quit = true;
                        break;
                    }
                }
            }
        }
    }

    async fn send_records<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        self.write_buf.clear();

        let records = &self.recv_buf[self.recv_handled..];
        let handled = self.formatter.serialize(records, &mut self.write_buf);
        if handled == 0 {
            warn!(
                "exporter {}: found too large piece when send data",
                self.config.exporter
            );
            // TODO add drop metrics
            self.recv_handled += 1;
            return Ok(());
        }

        writer.write_all_flush(&self.write_buf).await?;
        self.recv_handled += handled;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;

    use vey_types::metrics::NodeName;
    use vey_types::net::Host;

    use super::*;

    struct LineExport;

    impl StreamExport for LineExport {
        type Piece = &'static str;

        fn serialize(&self, pieces: &[&'static str], buf: &mut Vec<u8>) -> usize {
            for piece in pieces {
                buf.extend_from_slice(piece.as_bytes());
            }
            pieces.len()
        }
    }

    #[tokio::test]
    async fn connects_when_a_batch_is_ready() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::unbounded_channel();
        let mut config = StreamExportConfig::with_server(Host::localhost_v4(), port);
        config.check(NodeName::default()).unwrap();
        let rt = StreamExportRuntime::new(config, LineExport, rx);
        let run = tokio::spawn(async move { rt.into_running().await });

        let early = tokio::time::timeout(Duration::from_millis(200), listener.accept()).await;
        assert!(early.is_err(), "connected before a batch was ready");

        tx.send("metric 1 100\n").unwrap();
        let (mut sock, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
            .await
            .expect("accept")
            .unwrap();
        let mut buf = [0u8; 128];
        let n = tokio::time::timeout(Duration::from_secs(2), sock.read(&mut buf))
            .await
            .expect("write")
            .unwrap();
        assert_eq!(&buf[..n], b"metric 1 100\n");

        drop(tx);
        sock.shutdown().await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), run)
            .await
            .expect("runtime exit")
            .unwrap();
    }

    #[tokio::test]
    async fn retries_unsent_batch_after_write_error() {
        let mut rt = StreamExportRuntime::new(
            StreamExportConfig::new(2003),
            LineExport,
            mpsc::unbounded_channel().1,
        );
        rt.recv_buf.push("metric 1 100\n");

        let (client, server) = tokio::io::duplex(64);
        drop(server);
        let mut broken = client;
        assert!(rt.send_records(&mut broken).await.is_err());
        assert_eq!(rt.recv_handled, 0);

        let (mut client2, mut server2) = tokio::io::duplex(64);
        rt.send_records(&mut client2).await.unwrap();
        assert_eq!(rt.recv_handled, 1);

        let mut buf = [0u8; 64];
        let n = server2.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"metric 1 100\n");
    }
}
