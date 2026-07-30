/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use tokio::io::{AsyncRead, AsyncWrite, BufStream};

use vey_io_ext::LimitedBufReadExt;

use crate::config::FtpTransferConfig;
use crate::error::FtpLineDataReadError;

#[expect(async_fn_in_trait)]
pub trait FtpLineDataReceiver {
    async fn recv_line(&mut self, line: &str);
    fn should_return_early(&self) -> bool;
}

pub(crate) struct FtpLineDataTransfer<T: AsyncRead + AsyncWrite> {
    io: BufStream<T>,
    read_lines: usize,
    max_lines: usize,
    line_buf: Vec<u8>,
}

impl<T> FtpLineDataTransfer<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    pub(crate) fn new(io: T, config: &FtpTransferConfig) -> Self {
        FtpLineDataTransfer {
            io: BufStream::new(io),
            read_lines: 0,
            max_lines: config.list_max_entries,
            line_buf: Vec::with_capacity(config.list_max_line_len),
        }
    }

    async fn send_buf_to_receiver<R>(
        &mut self,
        receiver: &mut R,
    ) -> Result<(), FtpLineDataReadError>
    where
        R: FtpLineDataReceiver,
    {
        let s = std::str::from_utf8(&self.line_buf)
            .map_err(|_| FtpLineDataReadError::UnsupportedEncoding)?;
        receiver.recv_line(s).await;
        if receiver.should_return_early() {
            self.read_lines += 1;
            return Err(FtpLineDataReadError::AbortedByCallback);
        }
        self.line_buf.clear();
        Ok(())
    }

    pub(crate) async fn read_to_end<R>(
        mut self,
        receiver: &mut R,
    ) -> Result<(), FtpLineDataReadError>
    where
        R: FtpLineDataReceiver,
    {
        if !self.line_buf.is_empty() {
            self.send_buf_to_receiver(receiver).await?;
        }

        for i in self.read_lines..self.max_lines {
            let (found, nr) = self
                .io
                .limited_read_until(b'\n', self.line_buf.capacity(), &mut self.line_buf)
                .await?;
            if nr == 0 {
                return Ok(());
            }

            if !found {
                return Err(FtpLineDataReadError::LineTooLong(i + 1));
            }

            self.send_buf_to_receiver(receiver).await?;
        }

        Err(FtpLineDataReadError::TooManyLines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FtpTransferConfig;
    use std::io::Cursor;

    struct CollectLines {
        lines: Vec<String>,
        abort_after: Option<usize>,
    }

    impl FtpLineDataReceiver for CollectLines {
        async fn recv_line(&mut self, line: &str) {
            self.lines.push(line.to_owned());
        }

        fn should_return_early(&self) -> bool {
            self.abort_after.is_some_and(|n| self.lines.len() >= n)
        }
    }

    fn config(max_entries: usize, max_line_len: usize) -> FtpTransferConfig {
        FtpTransferConfig {
            list_max_entries: max_entries,
            list_max_line_len: max_line_len,
            ..FtpTransferConfig::default()
        }
    }

    #[tokio::test]
    async fn read_to_end_collects_lines() {
        let io = Cursor::new(b"one\ntwo\nthree\n".to_vec());
        let transfer = FtpLineDataTransfer::new(io, &config(16, 64));
        let mut receiver = CollectLines {
            lines: Vec::new(),
            abort_after: None,
        };
        transfer.read_to_end(&mut receiver).await.unwrap();
        assert_eq!(receiver.lines, ["one\n", "two\n", "three\n"]);
    }

    #[tokio::test]
    async fn read_to_end_rejects_too_many_lines() {
        let io = Cursor::new(b"a\nb\nc\n".to_vec());
        let transfer = FtpLineDataTransfer::new(io, &config(2, 64));
        let mut receiver = CollectLines {
            lines: Vec::new(),
            abort_after: None,
        };
        let err = transfer.read_to_end(&mut receiver).await.unwrap_err();
        assert!(matches!(err, FtpLineDataReadError::TooManyLines));
        assert_eq!(receiver.lines.len(), 2);
    }

    #[tokio::test]
    async fn read_to_end_rejects_line_too_long() {
        let io = Cursor::new(b"abcdefghij\n".to_vec());
        let transfer = FtpLineDataTransfer::new(io, &config(8, 4));
        let mut receiver = CollectLines {
            lines: Vec::new(),
            abort_after: None,
        };
        let err = transfer.read_to_end(&mut receiver).await.unwrap_err();
        assert!(matches!(err, FtpLineDataReadError::LineTooLong(1)));
    }

    #[tokio::test]
    async fn read_to_end_aborted_by_callback() {
        let io = Cursor::new(b"keep\nstop\nmore\n".to_vec());
        let transfer = FtpLineDataTransfer::new(io, &config(16, 64));
        let mut receiver = CollectLines {
            lines: Vec::new(),
            abort_after: Some(1),
        };
        let err = transfer.read_to_end(&mut receiver).await.unwrap_err();
        assert!(matches!(err, FtpLineDataReadError::AbortedByCallback));
        assert_eq!(receiver.lines, ["keep\n"]);
    }

    #[tokio::test]
    async fn read_to_end_rejects_non_utf8() {
        let io = Cursor::new(vec![0xff, 0xfe, b'\n']);
        let transfer = FtpLineDataTransfer::new(io, &config(8, 64));
        let mut receiver = CollectLines {
            lines: Vec::new(),
            abort_after: None,
        };
        let err = transfer.read_to_end(&mut receiver).await.unwrap_err();
        assert!(matches!(err, FtpLineDataReadError::UnsupportedEncoding));
    }
}
