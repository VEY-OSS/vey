/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::time::{Sleep, sleep};

/// Backoff before retrying a full async-job pool or a busy engine.
///
/// Waking the current task immediately turns those statuses into a spin.
pub(crate) const ASYNC_BUSY_RETRY: Duration = Duration::from_millis(1);

#[derive(Default)]
pub(crate) struct RetryLater {
    sleep: Option<Pin<Box<Sleep>>>,
}

impl RetryLater {
    pub(crate) fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if self.sleep.is_none() {
            self.sleep = Some(Box::pin(sleep(ASYNC_BUSY_RETRY)));
        }
        let Some(sleep) = self.sleep.as_mut() else {
            return Poll::Ready(());
        };
        match sleep.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(()) => {
                self.sleep = None;
                Poll::Ready(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::poll_fn;
    use std::task::Poll;

    use super::*;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
    }

    #[test]
    fn first_poll_waits_instead_of_waking_immediately() {
        let rt = runtime();
        rt.block_on(async {
            let mut retry = RetryLater::default();
            poll_fn(|cx| {
                assert!(matches!(retry.poll(cx), Poll::Pending));
                Poll::Ready(())
            })
            .await;
        });
    }

    #[test]
    fn retry_becomes_ready_after_the_backoff() {
        let rt = runtime();
        rt.block_on(async {
            let mut retry = RetryLater::default();
            poll_fn(|cx| retry.poll(cx)).await;
        });
    }
}
