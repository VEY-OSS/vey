/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::service::{IcapConnector, IcapServiceConfig};

use super::IcapConnectionPool;
use super::tests::test_icap_config;

#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
#[ignore]
async fn benchmark_get_during_refill() {
    const MIN_IDLE: usize = 4096;
    const GET_COUNT: usize = 1024;
    const PREFILL: usize = GET_COUNT;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

    let addr = listener.local_addr().unwrap();

    let accepted = Arc::new(AtomicUsize::new(0));
    let accepted_server = Arc::clone(&accepted);

    let server = tokio::spawn(async move {
        let mut connections = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            accepted_server.fetch_add(1, Ordering::Relaxed);
            connections.push(stream);
        }
    });

    let pool = Arc::new(make_test_pool(test_icap_config(
        addr,
        MIN_IDLE,
        MIN_IDLE,
        Duration::from_secs(60),
    )));

    for _ in 0..PREFILL {
        let conn = pool.connector.create().await.unwrap();
        assert!(pool.try_put(conn));
    }

    assert_eq!(pool.idle_pool.lock().unwrap().len(), PREFILL);

    let start_barrier = Arc::new(tokio::sync::Barrier::new(GET_COUNT + 2));
    let release_barrier = Arc::new(tokio::sync::Barrier::new(GET_COUNT + 1));

    let refill_pool = Arc::clone(&pool);
    let refill_start = Arc::clone(&start_barrier);

    let refill_task = tokio::spawn(async move {
        refill_start.wait().await;

        let start = Instant::now();

        refill_pool.refill().await;

        start.elapsed()
    });

    let rejected_returns = Arc::new(AtomicUsize::new(0));

    let mut get_tasks = Vec::with_capacity(GET_COUNT);

    for _ in 0..GET_COUNT {
        let pool = Arc::clone(&pool);
        let start_barrier = Arc::clone(&start_barrier);
        let release_barrier = Arc::clone(&release_barrier);
        let rejected_returns = Arc::clone(&rejected_returns);

        get_tasks.push(tokio::spawn(async move {
            start_barrier.wait().await;

            let start = Instant::now();

            let conn = pool.get().await.unwrap();

            let latency = start.elapsed();

            // Keep this connection checked out until every get() is done.
            release_barrier.wait().await;

            if !pool.try_put(conn) {
                rejected_returns.fetch_add(1, Ordering::Relaxed);
            }

            latency
        }));
    }

    // Release refill + all foreground get() calls.
    start_barrier.wait().await;

    // Wait till all get() calls are done.
    release_barrier.wait().await;

    let mut latencies = Vec::with_capacity(GET_COUNT);

    for task in get_tasks {
        latencies.push(task.await.unwrap());
    }

    let refill_time = refill_task.await.unwrap();

    latencies.sort_unstable();

    let p50 = percentile(&latencies, 0.50);
    let p95 = percentile(&latencies, 0.95);
    let p99 = percentile(&latencies, 0.99);

    tokio::time::sleep(Duration::from_millis(20)).await;

    let total_connections = accepted.load(Ordering::Relaxed);
    let rejected_returns = rejected_returns.load(Ordering::Relaxed);
    let final_idle = pool.idle_pool.lock().unwrap().len();

    println!("foreground gets:       {GET_COUNT}");
    println!("get p50:               {p50:?}");
    println!("get p95:               {p95:?}");
    println!("get p99:               {p99:?}");
    println!("refill time:           {refill_time:?}");
    println!("TCP connections:       {total_connections}");
    println!("rejected returns:      {rejected_returns}");
    println!("final idle pool:       {final_idle}");

    server.abort();
}

fn percentile(samples: &[Duration], p: f64) -> Duration {
    assert!(!samples.is_empty());
    let rank = (p * samples.len() as f64).ceil() as usize;
    samples[rank.saturating_sub(1).min(samples.len() - 1)]
}

fn make_test_pool(service_config: IcapServiceConfig) -> IcapConnectionPool {
    let config = Arc::new(service_config);
    let connector = Arc::new(IcapConnector::new(Arc::clone(&config)).unwrap());
    IcapConnectionPool::new(config, connector)
}
