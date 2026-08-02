/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use hdrhistogram::Histogram;
use tabled::builder::Builder;
use tabled::settings::object::Columns;
use tabled::settings::{Alignment, Padding, Style};
use tabled::{Table, Tabled};

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

#[derive(Tabled)]
pub(crate) struct HistRow {
    #[tabled(rename = "")]
    pub name: String,
    pub min: String,
    pub mean: String,
    #[tabled(rename = "+/-sd")]
    pub stdev: String,
    pub pct90: String,
    pub max: String,
}

#[derive(Tabled)]
struct PctRow {
    #[tabled(rename = "%")]
    pct: String,
    #[tabled(rename = "Time")]
    value: String,
}

#[derive(Tabled)]
struct TcpTrafficRow {
    #[tabled(rename = "")]
    direction: String,
    bytes: String,
    bps: String,
}

#[derive(Tabled)]
struct UdpTrafficRow {
    #[tabled(rename = "")]
    direction: String,
    bytes: String,
    packets: String,
    bps: String,
    pps: String,
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

    let mut table = Table::new(rows);
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
    let rows = [
        TcpTrafficRow {
            direction: "Send".to_string(),
            bytes: send_bytes.to_string(),
            bps: format!("{:.3}", send_bytes as f64 / total_secs),
        },
        TcpTrafficRow {
            direction: "Recv".to_string(),
            bytes: recv_bytes.to_string(),
            bps: format!("{:.3}", recv_bytes as f64 / total_secs),
        },
    ];

    println!("# Traffic");
    let mut table = Table::new(&rows);
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
    let rows = [
        UdpTrafficRow {
            direction: "Send".to_string(),
            bytes: send_bytes.to_string(),
            packets: send_packets.to_string(),
            bps: format!("{:.3}", send_bytes as f64 / total_secs),
            pps: format!("{:.3}", send_packets as f64 / total_secs),
        },
        UdpTrafficRow {
            direction: "Recv".to_string(),
            bytes: recv_bytes.to_string(),
            packets: recv_packets.to_string(),
            bps: format!("{:.3}", recv_bytes as f64 / total_secs),
            pps: format!("{:.3}", recv_packets as f64 / total_secs),
        },
    ];

    println!("# Traffic");
    let mut table = Table::new(&rows);
    table.modify(Columns::new(1..), Alignment::right());
    print_table(&mut table);
}

pub(crate) fn print_pct_table(h: &Histogram<u64>) {
    if h.len() <= 1 {
        return;
    }

    const PCTS: &[u8] = &[50, 66, 75, 80, 90, 95, 98, 99, 100];
    let rows: Vec<PctRow> = PCTS
        .iter()
        .map(|&pct| {
            let v = Duration::from_nanos(h.value_at_percentile(pct as f64));
            PctRow {
                pct: format!("{pct}%"),
                value: format!("{v:.3?}"),
            }
        })
        .collect();

    println!();
    println!("Percentage of the requests served within a certain time");
    let mut table = Table::new(&rows);
    table.modify(Columns::new(..), Alignment::right());
    table.with(Style::blank());
    table.with(Padding::new(1, 1, 0, 0));
    println!("{table}");
}
