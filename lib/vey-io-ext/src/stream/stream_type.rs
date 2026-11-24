/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use tokio::io::{AsyncRead, AsyncWrite, Join};
use tokio::net::{TcpStream, tcp};

pub trait AsyncStream: AsyncRead + AsyncWrite {
    type R: AsyncRead;
    type W: AsyncWrite;

    fn into_split(self) -> (Self::R, Self::W);
}

impl AsyncStream for TcpStream {
    type R = tcp::OwnedReadHalf;
    type W = tcp::OwnedWriteHalf;

    fn into_split(self) -> (Self::R, Self::W) {
        self.into_split()
    }
}

impl<R, W> AsyncStream for Join<R, W>
where
    R: AsyncRead,
    W: AsyncWrite,
{
    type R = R;
    type W = W;

    fn into_split(self) -> (Self::R, Self::W) {
        self.into_inner()
    }
}
