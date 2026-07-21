/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt;
use std::mem::ManuallyDrop;
use std::os::windows::io::{AsRawSocket, FromRawSocket};

use socket2::Socket;

use super::RawSocket;

impl fmt::Display for RawSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get_inner().as_raw_socket().fmt(f)
    }
}

impl Clone for RawSocket {
    fn clone(&self) -> Self {
        Self::from(self.get_inner())
    }
}

impl<T: AsRawSocket> From<&T> for RawSocket {
    fn from(value: &T) -> Self {
        let socket = unsafe { Socket::from_raw_socket(value.as_raw_socket()) };
        RawSocket {
            inner: ManuallyDrop::new(socket),
        }
    }
}
