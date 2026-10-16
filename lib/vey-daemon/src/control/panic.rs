/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::atomic::{AtomicBool, Ordering};

use log::warn;

use crate::opts::DaemonArgs;

static TRIGGER_PANIC_QUIT: AtomicBool = AtomicBool::new(true);

pub fn set_hook(args: &DaemonArgs) {
    if !args.panic_quit {
        return;
    }

    let monitored = args.monitored;
    std::panic::set_hook(Box::new(move |panic_info| {
        let panic_message = panic_info
            .payload_as_str()
            .unwrap_or("no panic payload string available");

        match std::thread::current().name() {
            Some(thread) => {
                if let Some(location) = panic_info.location() {
                    warn!(
                        "thread '{thread}' panicked at {}:{}:{}: {panic_message}",
                        location.file(),
                        location.line(),
                        location.column()
                    );
                } else {
                    warn!("thread '{thread}' panicked: {panic_message}");
                }
            }
            None => {
                if let Some(location) = panic_info.location() {
                    warn!(
                        "unknown thread panicked at {}:{}:{}: {panic_message}",
                        location.file(),
                        location.line(),
                        location.column()
                    );
                } else {
                    warn!("unknown thread panicked: {panic_message}");
                }
            }
        }

        let trigger_quit = TRIGGER_PANIC_QUIT.swap(false, Ordering::AcqRel);
        if !trigger_quit {
            return;
        }
        do_panic_quit(monitored);
    }));
}

#[cfg(unix)]
fn do_panic_quit(monitored: bool) {
    use rustix::process::Signal;

    let parent = rustix::process::getppid();
    let plan = plan_panic_quit(monitored, parent.is_some());
    // `getppid` is `Some` for every process that has a parent. Notifying that
    // parent must not skip this process's own shutdown.
    if plan.signal_parent
        && let Some(pid) = parent
    {
        let _ = rustix::process::kill_process(pid, Signal::HUP);
    }
    if plan.force_shutdown {
        crate::control::quit::trigger_force_shutdown();
    }
}

struct PanicQuitPlan {
    signal_parent: bool,
    force_shutdown: bool,
}

fn plan_panic_quit(monitored: bool, has_parent: bool) -> PanicQuitPlan {
    PanicQuitPlan {
        signal_parent: monitored && has_parent,
        force_shutdown: true,
    }
}

#[cfg(test)]
mod tests {
    use super::plan_panic_quit;

    #[test]
    fn monitored_panic_notifies_parent_and_still_shuts_down() {
        let plan = plan_panic_quit(true, true);
        assert!(plan.signal_parent);
        assert!(plan.force_shutdown);
    }

    #[test]
    fn panic_without_a_parent_only_shuts_down() {
        let plan = plan_panic_quit(true, false);
        assert!(!plan.signal_parent);
        assert!(plan.force_shutdown);

        let plan = plan_panic_quit(false, true);
        assert!(!plan.signal_parent);
        assert!(plan.force_shutdown);
    }
}

#[cfg(windows)]
fn do_panic_quit(_monitored: bool) {
    crate::control::quit::trigger_force_shutdown();
}
