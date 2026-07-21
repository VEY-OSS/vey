/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::IpAddr;

use tokio::sync::broadcast;

use vey_types::collection::{SelectiveItem, SelectivePickPolicy, SelectiveVec};
use vey_types::metrics::NodeName;

use super::ClientConnectionInfo;

#[derive(Clone)]
pub enum ServerReloadCommand<C: Clone> {
    QuitRuntime,
    UpdateInPlace(C),
    ReloadVersion(usize),
}

impl<C: Clone> ServerReloadCommand<C> {
    pub fn new_sender() -> broadcast::Sender<ServerReloadCommand<C>> {
        broadcast::Sender::new(64)
    }
}

pub trait BaseServer {
    fn name(&self) -> &NodeName;
    fn r#type(&self) -> &'static str;
    fn version(&self) -> usize;
}

pub trait ReloadServer {
    fn reload(&self) -> Self;
}

pub trait ServerExt: BaseServer {
    fn select_consistent<'a, T>(
        &self,
        nodes: &'a SelectiveVec<T>,
        pick_policy: SelectivePickPolicy,
        cc_info: &ClientConnectionInfo,
    ) -> &'a T
    where
        T: SelectiveItem,
    {
        #[derive(Hash)]
        struct ConsistentKey {
            client_ip: IpAddr,
            server_ip: IpAddr,
        }

        match pick_policy {
            SelectivePickPolicy::Random => nodes.pick_random(),
            SelectivePickPolicy::Serial => nodes.pick_serial(),
            SelectivePickPolicy::RoundRobin => nodes.pick_round_robin(),
            SelectivePickPolicy::Ketama => {
                let key = ConsistentKey {
                    client_ip: cc_info.client_ip(),
                    server_ip: cc_info.server_ip(),
                };
                nodes.pick_ketama(&key)
            }
            SelectivePickPolicy::Rendezvous => {
                let key = ConsistentKey {
                    client_ip: cc_info.client_ip(),
                    server_ip: cc_info.server_ip(),
                };
                nodes.pick_rendezvous(&key)
            }
            SelectivePickPolicy::JumpHash => {
                let key = ConsistentKey {
                    client_ip: cc_info.client_ip(),
                    server_ip: cc_info.server_ip(),
                };
                nodes.pick_jump(&key)
            }
        }
    }
}
