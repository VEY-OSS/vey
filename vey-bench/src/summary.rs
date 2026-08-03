/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use hdrhistogram::Histogram;
use tabled::Table;
use tabled::builder::Builder;
use tabled::settings::object::Columns;
use tabled::settings::{Alignment, Padding, Style};

#[derive(Clone)]
pub(crate) struct KvRow {
    pub key: String,
    pub value: String,
}

impl KvRow {
    pub(crate) fn new(key: impl Into<String>, value: impl ToString) -> Self {
        Self {
            key: key.into(),
            value: value.to_string(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct HistRow {
    pub name: String,
    pub min: String,
    pub mean: String,
    pub stdev: String,
    pub pct90: String,
    pub max: String,
}

fn print_table(table: &mut Table) {
    table.with(Style::blank());
    table.with(Padding::new(0, 1, 0, 0));
    println!("{table}");
}

pub(crate) fn print_split_line() {
    println!("---");
}

pub(crate) fn print_kv_section(title: &str, rows: &[KvRow]) {
    if rows.is_empty() {
        return;
    }

    if !title.is_empty() {
        println!("{title}");
    }

    let mut builder = Builder::with_capacity(rows.len(), 2);
    for row in rows {
        builder.push_record([&row.key, &row.value]);
    }
    let mut table = builder.build();
    print_table(&mut table);
}

pub(crate) fn print_hist_table(title: &str, rows: &[HistRow]) {
    if rows.is_empty() {
        return;
    }

    if !title.is_empty() {
        println!("{title}");
    }

    let mut builder = Builder::with_capacity(rows.len() + 1, 6);
    builder.push_record(["", "min", "mean", "+/-sd", "pct90", "max"]);
    for row in rows {
        builder.push_record([
            row.name.as_str(),
            row.min.as_str(),
            row.mean.as_str(),
            row.stdev.as_str(),
            row.pct90.as_str(),
            row.max.as_str(),
        ]);
    }
    let mut table = builder.build();
    table.modify(Columns::new(1..), Alignment::right());
    print_table(&mut table);
}

pub(crate) fn hist_row_from_data(name: &str, h: &Histogram<u64>) -> HistRow {
    HistRow {
        name: name.to_string(),
        min: h.min().to_string(),
        mean: format!("{:.3}", h.mean()),
        stdev: format!("{:.3}", h.stdev()),
        pct90: h.value_at_quantile(0.90).to_string(),
        max: h.max().to_string(),
    }
}

pub(crate) fn hist_row_from_duration(name: &str, h: &Histogram<u64>) -> HistRow {
    const NANOS_PER_SEC: f64 = 1_000_000_000.0;

    let t_min = Duration::from_nanos(h.min());
    let t_mean = Duration::from_secs_f64(h.mean() / NANOS_PER_SEC);
    let t_std_dev = Duration::from_secs_f64(h.stdev() / NANOS_PER_SEC);
    let t_pct90 = Duration::from_nanos(h.value_at_quantile(0.90));
    let t_max = Duration::from_nanos(h.max());

    HistRow {
        name: name.to_string(),
        min: format!("{t_min:.3?}"),
        mean: format!("{t_mean:.3?}"),
        stdev: format!("{t_std_dev:.3?}"),
        pct90: format!("{t_pct90:.3?}"),
        max: format!("{t_max:.3?}"),
    }
}

pub(crate) fn print_tcp_traffic(send_bytes: u64, recv_bytes: u64, total_secs: f64) {
    let mut builder = Builder::with_capacity(3, 3);
    builder.push_record(["", "bytes", "bps"]);
    builder.push_record([
        "Send",
        &send_bytes.to_string(),
        &format!("{:.3}", send_bytes as f64 / total_secs),
    ]);
    builder.push_record([
        "Recv",
        &recv_bytes.to_string(),
        &format!("{:.3}", recv_bytes as f64 / total_secs),
    ]);

    println!("# Traffic");
    let mut table = builder.build();
    table.modify(Columns::new(1..), Alignment::right());
    print_table(&mut table);
}

pub(crate) fn print_udp_traffic(
    send_bytes: u64,
    send_packets: u64,
    recv_bytes: u64,
    recv_packets: u64,
    total_secs: f64,
) {
    let mut builder = Builder::with_capacity(3, 5);
    builder.push_record(["", "bytes", "packets", "bps", "pps"]);
    builder.push_record([
        "Send",
        &send_bytes.to_string(),
        &send_packets.to_string(),
        &format!("{:.3}", send_bytes as f64 / total_secs),
        &format!("{:.3}", send_packets as f64 / total_secs),
    ]);
    builder.push_record([
        "Recv",
        &recv_bytes.to_string(),
        &recv_packets.to_string(),
        &format!("{:.3}", recv_bytes as f64 / total_secs),
        &format!("{:.3}", recv_packets as f64 / total_secs),
    ]);

    println!("# Traffic");
    let mut table = builder.build();
    table.modify(Columns::new(1..), Alignment::right());
    print_table(&mut table);
}

pub(crate) fn print_pct_table(h: &Histogram<u64>) {
    if h.len() <= 1 {
        return;
    }

    const PCTS: &[u8] = &[50, 66, 75, 80, 90, 95, 98, 99, 100];
    let mut builder = Builder::with_capacity(PCTS.len() + 1, 2);
    builder.push_record(["%", "Time"]);
    for &pct in PCTS {
        let v = Duration::from_nanos(h.value_at_percentile(pct as f64));
        builder.push_record([format!("{pct}%"), format!("{v:.3?}")]);
    }

    println!();
    println!("Percentage of the requests served within a certain time");
    let mut table = builder.build();
    table.modify(Columns::new(..), Alignment::right());
    table.with(Style::blank());
    table.with(Padding::new(1, 1, 0, 0));
    println!("{table}");
}
