/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::fmt;
use std::mem::ManuallyDrop;
use std::os::unix::io::{AsRawFd, FromRawFd};

use socket2::Socket;

use super::RawSocket;

impl Clone for RawSocket {
    fn clone(&self) -> Self {
        Self::from(self.get_inner())
    }
}

impl fmt::Display for RawSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get_inner().as_raw_fd().fmt(f)
    }
}

impl<T: AsRawFd> From<&T> for RawSocket {
    fn from(value: &T) -> Self {
        let socket = unsafe { Socket::from_raw_fd(value.as_raw_fd()) };
        RawSocket {
            inner: ManuallyDrop::new(socket),
        }
    }
}
