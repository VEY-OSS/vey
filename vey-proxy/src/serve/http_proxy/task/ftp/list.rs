/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{self, Write};

use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};

use vey_ftp_client::FtpLineDataReceiver;
use vey_io_ext::LimitedWriteExt;

const CHUNKED_BUF_HEAD_RESERVED: usize = (usize::BITS as usize >> 2) + 2;
const CHUNKED_BUF_TAIL_RESERVED: usize = 2;

pub(super) trait ListWriter: FtpLineDataReceiver {
    fn take_io_error(&mut self) -> Option<io::Error>;
    async fn flush_buf(&mut self) -> io::Result<()>;
    #[allow(unused)]
    fn is_idle(&self) -> bool;
    #[allow(unused)]
    fn reset_active(&mut self);
    #[allow(unused)]
    fn no_cached_data(&self) -> bool;
}

pub(super) struct ChunkedListWriter<'a, W> {
    buf_len: usize,
    buf_cap: usize,
    buf: Vec<u8>,
    writer: &'a mut W,
    io_error: Option<io::Error>,
    active: bool,
}

impl<'a, W> ChunkedListWriter<'a, W>
where
    W: AsyncWrite + Unpin,
{
    pub(super) fn new(writer: &'a mut W, buf_size: usize) -> Self {
        let mut buf =
            Vec::with_capacity(CHUNKED_BUF_HEAD_RESERVED + buf_size + CHUNKED_BUF_TAIL_RESERVED);
        buf.extend_from_slice(&[0u8; CHUNKED_BUF_HEAD_RESERVED]);
        ChunkedListWriter {
            buf_len: CHUNKED_BUF_HEAD_RESERVED,
            buf_cap: buf_size + CHUNKED_BUF_HEAD_RESERVED,
            buf,
            writer,
            io_error: None,
            active: false,
        }
    }

    async fn send_buf(&mut self) -> io::Result<()> {
        // Never emit a zero-sized data chunk here: that is the chunked terminator
        // and must only be written from flush_buf().
        if self.buf_len <= CHUNKED_BUF_HEAD_RESERVED {
            return Ok(());
        }

        let chunked_header = format!("{:x}\r\n", self.buf_len - CHUNKED_BUF_HEAD_RESERVED);
        let offset = CHUNKED_BUF_HEAD_RESERVED - chunked_header.len();
        let mut head = &mut self.buf[offset..];
        let _ = head.write_all(chunked_header.as_bytes());
        self.buf.extend_from_slice(b"\r\n");
        self.writer.write_all(&self.buf[offset..]).await?;

        self.buf_cap = self.buf.capacity() - CHUNKED_BUF_TAIL_RESERVED;
        self.buf_len = CHUNKED_BUF_HEAD_RESERVED;
        self.buf.truncate(self.buf_len);
        Ok(())
    }
}

impl<W> FtpLineDataReceiver for ChunkedListWriter<'_, W>
where
    W: AsyncWrite + Send + Unpin,
{
    async fn recv_line(&mut self, line: &str) {
        self.active = true;

        let mut remaining = line.as_bytes();
        while !remaining.is_empty() {
            let available = self.buf_cap.saturating_sub(self.buf_len);
            if available == 0 {
                if self.buf_len <= CHUNKED_BUF_HEAD_RESERVED {
                    self.io_error = Some(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "chunked list buffer is too small for FTP listing lines",
                    ));
                    return;
                }
                if let Err(e) = self.send_buf().await {
                    self.io_error = Some(e);
                    return;
                }
                continue;
            }

            let take = remaining.len().min(available);
            self.buf.extend_from_slice(&remaining[..take]);
            self.buf_len += take;
            remaining = &remaining[take..];

            if !remaining.is_empty()
                && let Err(e) = self.send_buf().await
            {
                self.io_error = Some(e);
                return;
            }
        }
    }

    #[inline]
    fn should_return_early(&self) -> bool {
        self.io_error.is_some()
    }
}

impl<W> ListWriter for ChunkedListWriter<'_, W>
where
    W: AsyncWrite + Send + Unpin,
{
    #[inline]
    fn take_io_error(&mut self) -> Option<io::Error> {
        self.io_error.take()
    }

    async fn flush_buf(&mut self) -> io::Result<()> {
        if self.buf_len > CHUNKED_BUF_HEAD_RESERVED {
            self.send_buf().await?;
        }
        self.writer.write_all_flush(b"0\r\n\r\n").await
    }

    #[inline]
    fn is_idle(&self) -> bool {
        !self.active
    }

    #[inline]
    fn reset_active(&mut self) {
        self.active = false;
    }

    #[inline]
    fn no_cached_data(&self) -> bool {
        self.buf_len <= CHUNKED_BUF_HEAD_RESERVED
    }
}

pub(super) struct EndingListWriter<'a, W> {
    writer: BufWriter<&'a mut W>,
    io_error: Option<io::Error>,
    active: bool,
}

impl<'a, W> EndingListWriter<'a, W>
where
    W: AsyncWrite + Unpin,
{
    pub(super) fn new(writer: &'a mut W, buf_size: usize) -> Self {
        EndingListWriter {
            writer: BufWriter::with_capacity(buf_size, writer),
            io_error: None,
            active: false,
        }
    }
}

impl<W> FtpLineDataReceiver for EndingListWriter<'_, W>
where
    W: AsyncWrite + Send + Unpin,
{
    async fn recv_line(&mut self, line: &str) {
        self.active = true;
        if let Err(e) = self.writer.write_all(line.as_bytes()).await {
            self.io_error = Some(e);
        }
    }

    #[inline]
    fn should_return_early(&self) -> bool {
        self.io_error.is_some()
    }
}

impl<W> ListWriter for EndingListWriter<'_, W>
where
    W: AsyncWrite + Send + Unpin,
{
    #[inline]
    fn take_io_error(&mut self) -> Option<io::Error> {
        self.io_error.take()
    }

    #[inline]
    async fn flush_buf(&mut self) -> io::Result<()> {
        self.writer.flush().await
    }

    #[inline]
    fn is_idle(&self) -> bool {
        !self.active
    }

    #[inline]
    fn reset_active(&mut self) {
        self.active = false;
    }

    #[inline]
    fn no_cached_data(&self) -> bool {
        self.writer.buffer().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    struct VecWriter {
        data: Vec<u8>,
    }

    impl AsyncWrite for VecWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.data.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    fn decode_chunked(body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut rest = body;
        loop {
            let crlf = rest
                .array_windows::<2>()
                .position(|w| w == b"\r\n")
                .expect("chunk size line");
            let size = usize::from_str_radix(std::str::from_utf8(&rest[..crlf]).unwrap(), 16)
                .expect("hex chunk size");
            rest = &rest[crlf + 2..];
            if size == 0 {
                assert_eq!(&rest[..2], b"\r\n");
                break;
            }
            out.extend_from_slice(&rest[..size]);
            assert_eq!(&rest[size..size + 2], b"\r\n");
            rest = &rest[size + 2..];
        }
        out
    }

    #[tokio::test]
    async fn long_line_is_split_without_early_terminator() {
        let mut writer = VecWriter { data: Vec::new() };
        // usable payload per chunk is 8 bytes
        let mut list = ChunkedListWriter::new(&mut writer, 8);
        list.recv_line("0123456789abcdef").await; // 16 bytes
        assert!(list.take_io_error().is_none());
        list.flush_buf().await.unwrap();

        let decoded = decode_chunked(&writer.data);
        assert_eq!(decoded, b"0123456789abcdef");
        // Must not contain an early terminating chunk before the final one.
        let body = String::from_utf8(writer.data.clone()).unwrap();
        assert_eq!(body.matches("0\r\n\r\n").count(), 1);
        assert!(body.ends_with("0\r\n\r\n"));
    }

    #[tokio::test]
    async fn empty_flush_only_writes_terminator() {
        let mut writer = VecWriter { data: Vec::new() };
        let mut list = ChunkedListWriter::new(&mut writer, 8);
        list.flush_buf().await.unwrap();
        assert_eq!(writer.data, b"0\r\n\r\n");
    }

    #[tokio::test]
    async fn normal_line_then_flush() {
        let mut writer = VecWriter { data: Vec::new() };
        let mut list = ChunkedListWriter::new(&mut writer, 64);
        list.recv_line("hello\r\n").await;
        list.flush_buf().await.unwrap();
        assert_eq!(decode_chunked(&writer.data), b"hello\r\n");
    }
}
