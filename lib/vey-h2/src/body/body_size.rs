/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use bytes::Bytes;
use http::{HeaderMap, Request, Response};
use tokio::io::BufReader;

use vey_io_ext::StreamCopyConfig;

use super::*;

type Duplex = tokio::io::DuplexStream;
type Client = h2::client::SendRequest<Bytes>;
type Server = h2::server::Connection<Duplex, Bytes>;
type Accepted = (
    http::Request<h2::RecvStream>,
    h2::server::SendResponse<Bytes>,
);

async fn handshake() -> (Client, Server) {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let ((send_request, conn), server) = tokio::join!(
        async {
            h2::client::handshake(client_io)
                .await
                .expect("client handshake")
        },
        async {
            h2::server::handshake(server_io)
                .await
                .expect("server handshake")
        },
    );
    tokio::spawn(async move {
        conn.await.expect("client connection");
    });
    (send_request, server)
}

fn spawn_server(mut server: Server) -> tokio::sync::oneshot::Receiver<Accepted> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let accepted = server.accept().await.expect("accept").expect("request");
        let _ = tx.send(accepted);
        while server.accept().await.is_some() {}
    });
    rx
}

fn get_request() -> Request<()> {
    Request::builder()
        .uri("http://example.com/")
        .body(())
        .unwrap()
}

async fn recv_all(body: &mut h2::RecvStream) -> Vec<u8> {
    let mut buf = Vec::new();
    while let Some(chunk) = body.data().await {
        let chunk = chunk.expect("recv data");
        let n = chunk.len();
        body.flow_control().release_capacity(n).unwrap();
        buf.extend_from_slice(&chunk);
    }
    buf
}

#[tokio::test]
async fn encode_counts_payload() {
    let payload = b"test\nbody";
    let (mut client, server) = handshake().await;
    let accepted = spawn_server(server);

    let server_got = tokio::spawn(async move {
        let (req, mut respond) = accepted.await.unwrap();
        respond
            .send_response(Response::new(()), true)
            .expect("send response");
        let mut body = req.into_body();
        recv_all(&mut body).await
    });

    let (resp_fut, mut send_stream) = client.send_request(get_request(), false).unwrap();
    let mut reader = tokio_test::io::Builder::new().read(payload).build();
    let mut encode =
        H2BodyEncodeTransfer::new(&mut reader, &mut send_stream, &StreamCopyConfig::default());
    (&mut encode).await.unwrap();
    assert_eq!(encode.copied_size(), payload.len() as u64);
    drop(encode);
    send_stream.send_data(Bytes::new(), true).unwrap();

    let _ = resp_fut.await.unwrap();
    let got = server_got.await.unwrap();
    assert_eq!(got, payload);
}

#[tokio::test]
async fn encode_empty_body_is_zero() {
    let (mut client, server) = handshake().await;
    let accepted = spawn_server(server);

    let server_got = tokio::spawn(async move {
        let (req, mut respond) = accepted.await.unwrap();
        respond
            .send_response(Response::new(()), true)
            .expect("send response");
        let mut body = req.into_body();
        recv_all(&mut body).await
    });

    let (resp_fut, mut send_stream) = client.send_request(get_request(), false).unwrap();
    let mut reader = tokio_test::io::Builder::new().build();
    let mut encode =
        H2BodyEncodeTransfer::new(&mut reader, &mut send_stream, &StreamCopyConfig::default());
    (&mut encode).await.unwrap();
    assert_eq!(encode.copied_size(), 0);
    drop(encode);
    send_stream.send_data(Bytes::new(), true).unwrap();

    let _ = resp_fut.await.unwrap();
    let got = server_got.await.unwrap();
    assert!(got.is_empty());
}

#[tokio::test]
async fn from_chunked_counts_decoded_payload() {
    let chunked = b"5\r\ntest\n\r\n4\r\nbody\r\n0\r\n\r\n";
    let (mut client, server) = handshake().await;
    let accepted = spawn_server(server);

    let server_got = tokio::spawn(async move {
        let (req, mut respond) = accepted.await.unwrap();
        respond
            .send_response(Response::new(()), true)
            .expect("send response");
        let mut body = req.into_body();
        recv_all(&mut body).await
    });

    let (resp_fut, mut send_stream) = client.send_request(get_request(), false).unwrap();
    let stream = tokio_test::io::Builder::new().read(chunked).build();
    let mut buf_stream = BufReader::new(stream);
    let mut xfer = H2StreamFromChunkedTransfer::new(
        &mut buf_stream,
        &mut send_stream,
        &StreamCopyConfig::default(),
        1024,
        1024,
    );
    (&mut xfer).await.unwrap();
    assert!(xfer.finished());
    assert_eq!(xfer.copied_size(), 9);

    let _ = resp_fut.await.unwrap();
    let got = server_got.await.unwrap();
    assert_eq!(got, b"test\nbody");
}

#[tokio::test]
async fn from_chunked_empty_and_trailer_are_not_payload() {
    let chunked = b"0\r\nA: B\r\n\r\n";
    let (mut client, server) = handshake().await;
    let accepted = spawn_server(server);

    let server_got = tokio::spawn(async move {
        let (req, mut respond) = accepted.await.unwrap();
        respond
            .send_response(Response::new(()), true)
            .expect("send response");
        let mut body = req.into_body();
        let data = recv_all(&mut body).await;
        let trailers = body.trailers().await.expect("trailers");
        (data, trailers)
    });

    let (resp_fut, mut send_stream) = client.send_request(get_request(), false).unwrap();
    let stream = tokio_test::io::Builder::new().read(chunked).build();
    let mut buf_stream = BufReader::new(stream);
    let mut xfer = H2StreamFromChunkedTransfer::new(
        &mut buf_stream,
        &mut send_stream,
        &StreamCopyConfig::default(),
        1024,
        1024,
    );
    (&mut xfer).await.unwrap();
    assert!(xfer.finished());
    assert_eq!(xfer.copied_size(), 0);

    let _ = resp_fut.await.unwrap();
    let (got, trailers) = server_got.await.unwrap();
    assert!(got.is_empty());
    let trailers = trailers.expect("expected trailers");
    assert_eq!(trailers.get("a").unwrap().as_bytes(), b"B");
}

#[tokio::test]
async fn to_chunked_counts_data_payload() {
    let payload = b"test\nbody";
    let (mut client, server) = handshake().await;
    let accepted = spawn_server(server);

    tokio::spawn(async move {
        let (_req, mut respond) = accepted.await.unwrap();
        let mut send = respond
            .send_response(Response::new(()), false)
            .expect("send response");
        send.reserve_capacity(payload.len());
        send.send_data(Bytes::copy_from_slice(payload), true)
            .unwrap();
    });

    let (resp_fut, _) = client.send_request(get_request(), true).unwrap();
    let resp = resp_fut.await.unwrap();
    let mut recv = resp.into_body();
    let mut write_buf = Vec::new();
    let mut xfer = H2StreamToChunkedTransfer::new(&mut recv, &mut write_buf, 1024);
    (&mut xfer).await.unwrap();
    assert!(xfer.finished());
    assert_eq!(xfer.copied_size(), payload.len() as u64);
    assert_eq!(&write_buf, b"9\r\ntest\nbody\r\n0\r\n\r\n");
}

#[tokio::test]
async fn to_chunked_empty_body_is_zero() {
    let (mut client, server) = handshake().await;
    let accepted = spawn_server(server);

    tokio::spawn(async move {
        let (_req, mut respond) = accepted.await.unwrap();
        respond
            .send_response(Response::new(()), true)
            .expect("send response");
    });

    let (resp_fut, _) = client.send_request(get_request(), true).unwrap();
    let resp = resp_fut.await.unwrap();
    let mut recv = resp.into_body();
    let mut write_buf = Vec::new();
    let mut xfer = H2StreamToChunkedTransfer::new(&mut recv, &mut write_buf, 1024);
    (&mut xfer).await.unwrap();
    assert!(xfer.finished());
    assert_eq!(xfer.copied_size(), 0);
    assert_eq!(&write_buf, b"0\r\n\r\n");
}

#[tokio::test]
async fn to_chunked_trailers_are_not_payload() {
    let (mut client, server) = handshake().await;
    let accepted = spawn_server(server);

    tokio::spawn(async move {
        let (_req, mut respond) = accepted.await.unwrap();
        let mut send = respond
            .send_response(Response::new(()), false)
            .expect("send response");
        send.reserve_capacity(5);
        send.send_data(Bytes::from_static(b"hello"), false).unwrap();
        let mut trailers = HeaderMap::new();
        trailers.insert("x-t", "v".parse().unwrap());
        send.send_trailers(trailers).unwrap();
    });

    let (resp_fut, _) = client.send_request(get_request(), true).unwrap();
    let resp = resp_fut.await.unwrap();
    let mut recv = resp.into_body();
    let mut write_buf = Vec::new();
    let mut xfer = H2StreamToChunkedTransfer::new(&mut recv, &mut write_buf, 1024);
    (&mut xfer).await.unwrap();
    assert!(xfer.finished());
    assert_eq!(xfer.copied_size(), 5);
    assert_eq!(&write_buf, b"5\r\nhello\r\n0\r\nx-t: v\r\n\r\n");
}

#[tokio::test]
async fn transfer_counts_data_payload() {
    let payload = b"hello";
    let (mut client, server) = handshake().await;
    let accepted = spawn_server(server);

    let copied = tokio::spawn(async move {
        let (req, mut respond) = accepted.await.unwrap();
        let send = respond
            .send_response(Response::new(()), false)
            .expect("send response");
        let mut xfer = H2BodyTransfer::new(req.into_body(), send, 1024);
        (&mut xfer).await.unwrap();
        xfer.copied_size()
    });

    let (resp_fut, mut send) = client.send_request(get_request(), false).unwrap();
    send.reserve_capacity(payload.len());
    send.send_data(Bytes::copy_from_slice(payload), true)
        .unwrap();

    let resp = resp_fut.await.unwrap();
    let mut recv = resp.into_body();
    let got = recv_all(&mut recv).await;
    assert_eq!(got, payload);
    assert_eq!(copied.await.unwrap(), payload.len() as u64);
}

#[tokio::test]
async fn transfer_empty_body_is_zero() {
    let (mut client, server) = handshake().await;
    let accepted = spawn_server(server);

    let copied = tokio::spawn(async move {
        let (req, mut respond) = accepted.await.unwrap();
        let send = respond
            .send_response(Response::new(()), false)
            .expect("send response");
        let mut xfer = H2BodyTransfer::new(req.into_body(), send, 1024);
        (&mut xfer).await.unwrap();
        xfer.copied_size()
    });

    let (resp_fut, _) = client.send_request(get_request(), true).unwrap();
    let resp = resp_fut.await.unwrap();
    let mut recv = resp.into_body();
    let got = recv_all(&mut recv).await;
    assert!(got.is_empty());
    assert_eq!(copied.await.unwrap(), 0);
}
