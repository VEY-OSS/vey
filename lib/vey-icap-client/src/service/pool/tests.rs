use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use url::Url;

use std::sync::Arc;

use crate::service::{IcapClientConnection, IcapConnector, IcapServiceConfig};

use super::{IcapConnectionPool, IdleIcapConnection};

fn dummy_connection() -> (IcapClientConnection, DuplexStream) {
    let (client, server) = tokio::io::duplex(1024);
    let (reader, writer) = tokio::io::split(client);

    (IcapClientConnection::new(reader, writer), server)
}

pub(super) fn test_icap_config(
    addr: SocketAddr,
    min: usize,
    max: usize,
    timeout: Duration,
) -> IcapServiceConfig {
    let mut icap_config = IcapServiceConfig::new(
        crate::IcapMethod::Reqmod,
        Url::parse(&format!("icap://{addr}",)).unwrap(),
    )
    .unwrap();

    icap_config.connection_pool.set_min_idle_count(min);
    icap_config.connection_pool.set_max_idle_count(max);
    icap_config.connection_pool.set_idle_timeout(timeout);
    icap_config.tcp_connect_timeout = Duration::from_secs(10);
    icap_config
}

fn make_dummy_pool(min: usize, max: usize) -> IcapConnectionPool {
    let addr: SocketAddr = "127.0.0.1:1344".parse().unwrap();
    let config = Arc::new(test_icap_config(addr, min, max, Duration::from_secs(5)));

    let connector = Arc::new(IcapConnector::new(Arc::clone(&config)).unwrap());

    IcapConnectionPool::new(config, connector)
}

#[test]
fn try_put_accepts_clean_connection() {
    let pool = make_dummy_pool(0, 1);
    let (conn, _server) = dummy_connection();

    assert!(conn.reusable());

    assert!(pool.try_put(conn));

    assert_eq!(pool.idle_pool.lock().unwrap().len(), 1);
}

#[test]
fn try_put_rejects_dirty_connection() {
    let pool = make_dummy_pool(0, 1);
    let (mut conn, _server) = dummy_connection();

    conn.mark_io_inuse();

    assert!(!conn.reusable());
    assert!(!pool.try_put(conn));

    assert_eq!(pool.idle_pool.lock().unwrap().len(), 0);
}

#[test]
fn try_put_respects_max_idle() {
    let pool = make_dummy_pool(1, 2);

    // Assuming test config has max_idle_count == 2.
    let (conn1, _server1) = dummy_connection();
    let (conn2, _server2) = dummy_connection();
    let (conn3, _server3) = dummy_connection();

    assert!(pool.try_put(conn1));
    assert!(pool.try_put(conn2));

    // Pool is full.
    assert!(!pool.try_put(conn3));

    assert_eq!(pool.idle_pool.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn take_uses_lifo_order() {
    let pool = make_dummy_pool(1, 2);

    let (conn1, mut server1) = dummy_connection();
    let (conn2, mut server2) = dummy_connection();

    assert!(pool.try_put(conn1));
    assert!(pool.try_put(conn2));

    // Give each connection a recognizable marker.
    server1.write_all(b"A").await.unwrap();
    server2.write_all(b"B").await.unwrap();

    // conn2 was inserted last, so it should come out first.
    let mut conn = pool.take().unwrap();

    let mut buf = [0u8; 1];
    conn.reader.read_exact(&mut buf).await.unwrap();

    assert_eq!(&buf, b"B");
}

#[test]
fn expire_removes_old_connections() {
    let pool = make_dummy_pool(1, 2);

    let idle_timeout = pool.config.connection_pool.idle_timeout();

    let (old_conn, _old_server) = dummy_connection();
    let (new_conn, _new_server) = dummy_connection();

    let now = Instant::now();

    {
        let mut idle = pool.idle_pool.lock().unwrap();

        idle.push_back(IdleIcapConnection {
            conn: old_conn,
            idle_since: now - idle_timeout - Duration::from_secs(1),
        });

        idle.push_back(IdleIcapConnection {
            conn: new_conn,
            idle_since: now,
        });
    }

    assert_eq!(pool.idle_pool.lock().unwrap().len(), 2);

    let expired = pool.expire();

    assert_eq!(expired, 1);

    assert_eq!(pool.idle_pool.lock().unwrap().len(), 1);
}
