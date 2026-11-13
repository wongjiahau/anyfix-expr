//! `BYTES` values: base64 in a string on the wire.

mod common;

use anyx_protocol::{
    Call, Outcome, Response, Value, decode_request, decode_response, encode_request,
    encode_response,
};
use common::schema;

const FILES: &str = "PROTOCOL a VERSION 1,
METHOD export (path: TEXT) RETURNS (file: BYTES),
METHOD upload (file: BYTES) RETURNS (size: INTEGER),";

/// Every byte value, so no byte is lost on the way.
fn every_byte() -> Vec<u8> {
    (0..=255).collect()
}

#[test]
fn bytes_read_back_as_they_were_written() {
    let schema = schema(FILES);
    let calls = vec![Call {
        method: 1,
        arguments: vec![Value::Bytes(every_byte())],
    }];
    let request = encode_request(&schema, &calls).unwrap();
    assert_eq!(decode_request(&schema, &request).unwrap(), calls);

    let response = Response::Outcomes(vec![Outcome::Value(Value::Bytes(every_byte()))]);
    let text = encode_response(&schema, &[0], &response).unwrap();
    assert_eq!(decode_response(&schema, &[0], &text).unwrap(), response);
}

#[test]
fn bytes_are_written_as_base64() {
    let schema = schema(FILES);
    let response = Response::Outcomes(vec![Outcome::Value(Value::Bytes(b"MThd".to_vec()))]);
    assert_eq!(
        encode_response(&schema, &[0], &response).unwrap(),
        "\"TVRoZA==\"\n"
    );
}

#[test]
fn a_string_that_is_not_base64_is_a_mistake() {
    let schema = schema(FILES);
    let problem = decode_request(&schema, "upload \"not base64!\"").unwrap_err();
    assert!(
        problem.message.contains("expected base64 in the string"),
        "{}",
        problem.message
    );
    let problem = decode_request(&schema, "upload 3").unwrap_err();
    assert!(
        problem.message.contains("expected base64 in a string"),
        "{}",
        problem.message
    );
}
