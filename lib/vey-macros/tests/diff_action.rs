/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::cell::Cell;
use std::ops::BitOrAssign;

use vey_macros::DiffAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Diff {
    Same,
    Spawn,
    Reload,
    Respawn,
    Update(u64),
}

enum AnyCfg {
    Tcp(TcpCfg),
    Quic(QuicCfg),
    Fixed(FixedCfg),
}

#[derive(PartialEq, Eq, DiffAction)]
#[diff_action(
    new = AnyCfg,
    variant = Tcp,
    mismatch = Diff::Spawn,
    unchanged = Diff::Same,
    changed = Diff::Reload,
)]
struct TcpCfg {
    name: Counted,
    #[diff(changed = Diff::Respawn)]
    listen: Counted,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Flags(u64);

impl Flags {
    const LISTEN: Self = Self(0b0001);
    const FILTER: Self = Self(0b0010);

    fn empty() -> Self {
        Self(0)
    }

    fn bits(self) -> u64 {
        self.0
    }
}

impl BitOrAssign for Flags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

struct Listen {
    respawn: u8,
    reload: u8,
    respawn_hits: Cell<u32>,
    reload_hits: Cell<u32>,
}

impl Listen {
    fn new(respawn: u8, reload: u8) -> Self {
        Listen {
            respawn,
            reload,
            respawn_hits: Cell::new(0),
            reload_hits: Cell::new(0),
        }
    }

    fn need_respawn(&self, other: &Self) -> bool {
        self.respawn_hits.set(self.respawn_hits.get() + 1);
        self.respawn != other.respawn
    }

    fn need_reload(&self, other: &Self) -> bool {
        self.reload_hits.set(self.reload_hits.get() + 1);
        self.reload != other.reload
    }
}

struct Counted {
    value: u8,
    hits: Cell<u32>,
}

impl PartialEq for Counted {
    fn eq(&self, other: &Self) -> bool {
        self.hits.set(self.hits.get() + 1);
        self.value == other.value
    }
}

impl Eq for Counted {}

impl Counted {
    fn new(value: u8) -> Self {
        Counted {
            value,
            hits: Cell::new(0),
        }
    }
}

#[derive(DiffAction)]
#[diff_action(
    new = AnyCfg,
    variant = Quic,
    mismatch = Diff::Spawn,
    unchanged = Diff::Same,
    changed = Diff::Update,
    flags = Flags,
)]
struct QuicCfg {
    #[diff(cmp = need_respawn, changed = Diff::Respawn)]
    #[diff(cmp = need_reload, flag = Flags::LISTEN)]
    listen: Listen,
    #[diff(flag = Flags::FILTER)]
    filter: Counted,
    name: Counted,
}

#[derive(DiffAction)]
#[diff_action(
    new = AnyCfg,
    variant = Fixed,
    mismatch = Diff::Spawn,
    changed = Diff::Reload,
)]
struct FixedCfg {
    #[allow(dead_code)]
    name: u8,
}

fn tcp(name: u8, listen: u8) -> TcpCfg {
    TcpCfg {
        name: Counted::new(name),
        listen: Counted::new(listen),
    }
}

fn quic(respawn: u8, reload: u8, filter: u8, name: u8) -> QuicCfg {
    QuicCfg {
        listen: Listen::new(respawn, reload),
        filter: Counted::new(filter),
        name: Counted::new(name),
    }
}

#[test]
fn type_mismatch_does_not_read_fields() {
    let old = tcp(1, 2);
    let other = AnyCfg::Fixed(FixedCfg { name: 1 });
    assert_eq!(old.diff_action(&other), Diff::Spawn);
    assert_eq!(old.listen.hits.get(), 0);
    assert_eq!(old.name.hits.get(), 0);
}

#[test]
fn equal_configs_compare_each_field_once() {
    let old = tcp(1, 2);
    let new = tcp(1, 2);
    assert_eq!(old.diff_action(&AnyCfg::Tcp(new)), Diff::Same);
    assert_eq!(old.listen.hits.get(), 1);
    assert_eq!(old.name.hits.get(), 1);
}

#[test]
fn respawn_field_is_not_compared_again() {
    let old = tcp(1, 2);
    let new = tcp(9, 3);
    assert_eq!(old.diff_action(&AnyCfg::Tcp(new)), Diff::Respawn);
    assert_eq!(old.listen.hits.get(), 1);
    assert_eq!(old.name.hits.get(), 0);
}

#[test]
fn other_field_change_uses_the_default_action() {
    let old = tcp(1, 2);
    let new = tcp(9, 2);
    assert_eq!(old.diff_action(&AnyCfg::Tcp(new)), Diff::Reload);
    assert_eq!(old.listen.hits.get(), 1);
    assert_eq!(old.name.hits.get(), 1);
}

#[test]
fn split_listen_parts_are_disjoint() {
    let old = quic(1, 2, 3, 4);
    let respawn = quic(9, 2, 3, 4);
    assert_eq!(old.diff_action(&AnyCfg::Quic(respawn)), Diff::Respawn);
    assert_eq!(old.listen.respawn_hits.get(), 1);
    assert_eq!(old.listen.reload_hits.get(), 0);
    assert_eq!(old.filter.hits.get(), 0);
    assert_eq!(old.name.hits.get(), 0);

    let old = quic(1, 2, 3, 4);
    let reload = quic(1, 8, 3, 4);
    assert_eq!(
        old.diff_action(&AnyCfg::Quic(reload)),
        Diff::Update(Flags::LISTEN.bits())
    );
    assert_eq!(old.listen.respawn_hits.get(), 1);
    assert_eq!(old.listen.reload_hits.get(), 1);
    assert_eq!(old.filter.hits.get(), 1);
    assert_eq!(old.name.hits.get(), 1);

    let old = quic(1, 2, 3, 4);
    let both_flags = quic(1, 2, 7, 5);
    assert_eq!(
        old.diff_action(&AnyCfg::Quic(both_flags)),
        Diff::Update(Flags::FILTER.bits())
    );
    assert_eq!(old.listen.respawn_hits.get(), 1);
    assert_eq!(old.listen.reload_hits.get(), 1);
    assert_eq!(old.filter.hits.get(), 1);
    assert_eq!(old.name.hits.get(), 1);
}

#[test]
fn fixed_variant_skips_field_comparison() {
    let old = FixedCfg { name: 1 };
    assert_eq!(
        old.diff_action(&AnyCfg::Fixed(FixedCfg { name: 2 })),
        Diff::Reload
    );
    assert_eq!(old.diff_action(&AnyCfg::Tcp(tcp(1, 2))), Diff::Spawn);
}
