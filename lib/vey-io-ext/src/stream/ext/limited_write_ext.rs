/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::IoSlice;

use tokio::io::AsyncWrite;

use super::write_all_flush::WriteAllFlush;
use super::write_all_vectored::WriteAllVectored;

pub trait LimitedWriteExt: AsyncWrite {
    fn write_all_vectored<'a, 'b, const N: usize>(
        &'a mut self,
        bufs: [IoSlice<'b>; N],
    ) -> WriteAllVectored<'a, 'b, Self, N>
    where
        Self: Unpin,
    {
        WriteAllVectored::new(self, bufs)
    }

    fn write_all_flush<'a>(&'a mut self, buf: &'a [u8]) -> WriteAllFlush<'a, Self>
    where
        Self: Unpin,
    {
        WriteAllFlush::new(self, buf)
    }
}

impl<W: AsyncWrite + ?Sized> LimitedWriteExt for W {}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn write_all_flush_writes_and_flushes() {
        let mut writer = Vec::new();
        writer.write_all_flush(b"abcdef").await.unwrap();
        assert_eq!(writer, b"abcdef");
    }

    #[tokio::test]
    async fn write_all_vectored_writes_all_slices() {
        let mut writer = Vec::new();
        let bufs = [
            IoSlice::new(b"ab"),
            IoSlice::new(b"cd"),
            IoSlice::new(b"ef"),
        ];
        writer.write_all_vectored(bufs).await.unwrap();
        // Vec's AsyncWrite may not implement vectored specially; still should complete.
        writer.flush().await.unwrap();
        assert_eq!(writer, b"abcdef");
    }
}
