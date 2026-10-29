/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::os::fd::RawFd;
use std::task::{Context, Poll};

use tokio::io::unix::AsyncFd;

/// Poll tracked engine fds. A readable fd has its cached readiness cleared so
/// the next wait observes a new edge after OpenSSL drains the descriptor.
pub(crate) fn poll_readable(
    tracked_fds: &mut [AsyncFd<RawFd>],
    cx: &mut Context<'_>,
) -> Poll<io::Result<()>> {
    for fd in tracked_fds {
        match fd.poll_read_ready(cx) {
            Poll::Pending => {}
            Poll::Ready(Ok(mut guard)) => {
                guard.clear_ready();
                return Poll::Ready(Ok(()));
            }
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
        }
    }
    Poll::Pending
}

#[cfg(test)]
mod tests {
    use std::future::poll_fn;

    use tokio::io::Interest;

    use super::*;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .unwrap()
    }

    #[test]
    fn read_ready_is_not_cached_after_the_byte_is_consumed() {
        let rt = runtime();
        rt.block_on(async {
            let mut fds = [0; 2];
            assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
            for fd in fds {
                let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
                assert!(flags >= 0);
                assert_eq!(
                    unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) },
                    0
                );
            }
            let read_fd = fds[0];
            let write_fd = fds[1];
            assert_eq!(unsafe { libc::write(write_fd, b"x".as_ptr().cast(), 1) }, 1);

            let async_fd =
                unsafe { AsyncFd::register_with_interest(read_fd, Interest::READABLE) }.unwrap();
            let mut tracked = vec![async_fd];
            poll_fn(|cx| poll_readable(&mut tracked, cx)).await.unwrap();

            let mut buf = [0u8; 1];
            assert_eq!(
                unsafe { libc::read(read_fd, buf.as_mut_ptr().cast(), 1) },
                1
            );

            let polled = poll_fn(|cx| Poll::Ready(poll_readable(&mut tracked, cx))).await;
            assert!(matches!(polled, Poll::Pending));

            drop(tracked);
            unsafe {
                libc::close(read_fd);
                libc::close(write_fd);
            }
        });
    }
}
