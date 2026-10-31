/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod quit;
pub use quit::QuitActor;

mod upgrade;
pub use upgrade::UpgradeActor;

mod local;
pub use local::{DaemonController, UniqueController};

pub mod capnp;
