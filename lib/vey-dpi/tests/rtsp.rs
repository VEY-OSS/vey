/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use vey_dpi::{
    MaybeProtocol, Protocol, ProtocolInspectError, ProtocolInspectionConfig, ProtocolInspector,
};

// Helper function to test RTSP protocol detection
fn check_rtsp_data(data: &[u8]) -> Result<Protocol, ProtocolInspectError> {
    let mut inspector = ProtocolInspector::default();
    inspector.push_protocol(MaybeProtocol::Rtsp);
    let config = ProtocolInspectionConfig::default();
    inspector.check_client_initial_data(&config, 0, data)
}

#[test]
fn check_rtsp_client_setup_request_valid_rtsp_setup_request() {
    // Valid RTSP SETUP request with minimum required length
    let data = b"SETUP rtsp://example.com/media.mp4 RTSP/1.0\r\n";
    let p = check_rtsp_data(data).expect("Valid RTSP should be detected");
    assert_eq!(p, Protocol::Rtsp);
}

#[test]
fn check_rtsp_client_setup_request_insufficient_data_length() {
    // Only 23 bytes (less than required 25)
    let data = b"SETUP rtsp://example.co";
    let err = check_rtsp_data(data).expect_err("Should return NeedMoreData error");

    let ProtocolInspectError::NeedMoreData(needed) = err;
    assert_eq!(needed, 2);
}

#[test]
fn check_rtsp_client_setup_request_describe() {
    let data = b"DESCRIBE rtsp://example.com/media.mp4 RTSP/1.0\r\n";
    let p = check_rtsp_data(data).expect("DESCRIBE should be detected");
    assert_eq!(p, Protocol::Rtsp);
}

#[test]
fn check_rtsp_client_setup_request_options_star() {
    let data = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n";
    let p = check_rtsp_data(data).expect("OPTIONS * should be detected");
    assert_eq!(p, Protocol::Rtsp);
}

#[test]
fn check_rtsp_client_setup_request_options_uri() {
    let data = b"OPTIONS rtsp://example.com/media.mp4 RTSP/1.0\r\n";
    let p = check_rtsp_data(data).expect("OPTIONS rtsp URI should be detected");
    assert_eq!(p, Protocol::Rtsp);
}

#[test]
fn check_rtsp_client_setup_request_invalid_first_byte() {
    // Not an RTSP method
    let data = b"TESTUP rtsp://example.com/media.mp4 RTSP/1.0\r\n";
    let p = check_rtsp_data(data).expect("Should return Protocol::Unknown");
    assert_eq!(p, Protocol::Unknown);
}

#[test]
fn check_rtsp_client_setup_request_invalid_prefix() {
    // Valid first byte 'S' but invalid prefix
    let data = b"SOMETHING rtsp://example.com/media.mp4 RTSP/1.0\r\n";
    let p = check_rtsp_data(data).expect("Should return Protocol::Unknown");
    assert_eq!(p, Protocol::Unknown);
}

#[test]
fn check_rtsp_client_setup_request_exact_min_length_valid() {
    // Exactly 25 bytes with valid prefix
    let mut data = [0u8; 25];
    data[..13].copy_from_slice(b"SETUP rtsp://");
    let p = check_rtsp_data(&data).expect("Valid RTSP should be detected");
    assert_eq!(p, Protocol::Rtsp);
}

#[test]
fn check_rtsp_client_setup_request_exact_min_length_invalid() {
    // Exactly 25 bytes with invalid prefix
    let mut data = [0u8; 25];
    data[..13].copy_from_slice(b"SETUP ftp:// ");
    let p = check_rtsp_data(&data).expect("Should return Protocol::Unknown");
    assert_eq!(p, Protocol::Unknown);
}

#[test]
fn describe_survives_when_http_is_checked_first() {
    let mut inspector = ProtocolInspector::default();
    inspector.push_protocol(MaybeProtocol::Rtsp);
    inspector.push_protocol(MaybeProtocol::Http);
    let config = ProtocolInspectionConfig::default();
    let data = b"DESCRIBE rtsp://example.com/media.mp4 RTSP/1.0\r\n";
    let p = inspector
        .check_client_initial_data(&config, 0, data)
        .expect("DESCRIBE should be detected");
    assert_eq!(p, Protocol::Rtsp);
}

#[test]
fn get_parameter_survives_when_http_is_checked_first() {
    let mut inspector = ProtocolInspector::default();
    inspector.push_protocol(MaybeProtocol::Rtsp);
    inspector.push_protocol(MaybeProtocol::Http);
    let config = ProtocolInspectionConfig::default();
    let data = b"GET_PARAMETER rtsp://example.com/media.mp4 RTSP/1.0\r\n";
    let p = inspector
        .check_client_initial_data(&config, 0, data)
        .expect("GET_PARAMETER should be detected");
    assert_eq!(p, Protocol::Rtsp);
}

#[test]
fn options_star_survives_when_http_is_checked_first() {
    let mut inspector = ProtocolInspector::default();
    inspector.push_protocol(MaybeProtocol::Rtsp);
    inspector.push_protocol(MaybeProtocol::Http);
    let config = ProtocolInspectionConfig::default();
    let data = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n";
    let p = inspector
        .check_client_initial_data(&config, 0, data)
        .expect("OPTIONS * should be detected");
    assert_eq!(p, Protocol::Rtsp);
}

#[test]
fn describe_short_read_on_rtsp_port() {
    let mut inspector = ProtocolInspector::default();
    let config = ProtocolInspectionConfig::default();
    let data = b"DESCRIBE rtsp://example.com/media.mp4 RTSP/1.0\r\n";
    let err = inspector
        .check_client_initial_data(&config, 554, &data[..20])
        .expect_err("short DESCRIBE needs more data");
    assert!(matches!(err, ProtocolInspectError::NeedMoreData(_)));

    let p = inspector
        .check_client_initial_data(&config, 554, data)
        .expect("DESCRIBE should be detected");
    assert_eq!(p, Protocol::Rtsp);
}

#[test]
fn options_star_short_read_on_rtsp_port() {
    let mut inspector = ProtocolInspector::default();
    let config = ProtocolInspectionConfig::default();
    let data = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n";
    let err = inspector
        .check_client_initial_data(&config, 554, &data[..20])
        .expect_err("short OPTIONS needs more data");
    assert!(matches!(err, ProtocolInspectError::NeedMoreData(_)));

    let p = inspector
        .check_client_initial_data(&config, 554, data)
        .expect("OPTIONS * should be detected");
    assert_eq!(p, Protocol::Rtsp);
}

#[test]
fn http_options_on_rtsp_port_stays_http() {
    let mut inspector = ProtocolInspector::default();
    let config = ProtocolInspectionConfig::default();
    let data = b"OPTIONS / HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let p = inspector
        .check_client_initial_data(&config, 554, data)
        .expect("HTTP OPTIONS should be detected");
    assert_eq!(p, Protocol::Http1);
}

#[test]
fn http_get_on_rtsp_port_stays_http() {
    let mut inspector = ProtocolInspector::default();
    let config = ProtocolInspectionConfig::default();
    let data = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let p = inspector
        .check_client_initial_data(&config, 554, data)
        .expect("HTTP GET should be detected");
    assert_eq!(p, Protocol::Http1);
}

#[test]
fn check_rtsp_client_setup_request_protocol_exclusion_verification() {
    // Create inspector with multiple protocols
    let mut inspector = ProtocolInspector::default();
    inspector.push_protocol(MaybeProtocol::Ssl);
    inspector.push_protocol(MaybeProtocol::Http);
    inspector.push_protocol(MaybeProtocol::Rtsp);
    inspector.push_protocol(MaybeProtocol::Mqtt);

    let config = ProtocolInspectionConfig::default();
    let data = b"SETUP rtsp://example.com/media.mp4 RTSP/1.0\r\n";

    // Process valid RTSP data
    let result = inspector
        .check_client_initial_data(&config, 0, data)
        .expect("Should detect RTSP protocol");

    // Verify only RTSP was detected (others should be excluded)
    assert_eq!(result, Protocol::Rtsp);
}
