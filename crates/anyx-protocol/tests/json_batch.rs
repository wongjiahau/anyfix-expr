//! Several calls in one request, as JSON.

mod common;

use anyx_protocol::json::{
    decode_request_json, decode_responses_json, encode_requests_json, encode_response_json,
};
use common::poentaro;

const CALLS: &str = r#"[{"arguments":["a"],"method":"read _"},{"arguments":["b"],"method":"read _"},{"arguments":["c",3,"x"],"method":"write _ at revision _ content _"}]"#;
const METHODS: &str = r#"["read _","read _","write _ at revision _ content _"]"#;

#[test]
fn calls_are_written_as_one_request_and_read_back() {
    let schema = poentaro();
    let request = encode_requests_json(&schema, CALLS).unwrap();
    assert_eq!(decode_request_json(&schema, &request).unwrap(), CALLS);
}

#[test]
fn a_call_the_schema_lacks_is_named() {
    let schema = poentaro();
    let mistake = encode_requests_json(&schema, r#"[{"method":"fly","arguments":[]}]"#);
    assert!(mistake.unwrap_err().contains("fly"));
    let unnamed = encode_requests_json(&schema, r#"[{"arguments":[]}]"#);
    assert!(unnamed.unwrap_err().contains("\"method\""));
}

#[test]
fn each_call_of_a_response_reads_as_its_own_outcome() {
    let schema = poentaro();
    let request = encode_requests_json(&schema, CALLS).unwrap();
    let outcomes = r#"[{"error":{"kind":"trashed"},"kind":"error"},{"kind":"internalError"},{"kind":"notRun"}]"#;
    let response = encode_response_json(&schema, &request, outcomes).unwrap();
    assert_eq!(
        decode_responses_json(&schema, METHODS, &response).unwrap(),
        outcomes
    );
}

#[test]
fn an_unreadable_request_answers_every_call() {
    let schema = poentaro();
    let invalid = r#"{"column":2,"kind":"invalidRequest","line":1,"message":"m"}"#;
    assert_eq!(
        decode_responses_json(
            &schema,
            r#"["read _","read _"]"#,
            "INVALID REQUEST { line 1, column 2, message \"m\" }"
        )
        .unwrap(),
        format!("[{invalid},{invalid}]")
    );
}
