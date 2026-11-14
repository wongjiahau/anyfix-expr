//! Values as JSON, for the TypeScript client.

mod common;

use anyx_protocol::json::{
    decode_request_json, decode_response_json, encode_request_json, encode_response_json,
};
use common::poentaro;

const WRITE: &str = "write _ at revision _ content _";

#[test]
fn a_request_is_written_from_json_arguments() {
    let schema = poentaro();
    let request = encode_request_json(&schema, WRITE, r#"["a", 3, "x"]"#).unwrap();
    assert_eq!(request, "write \"a\" at revision 3 content \"x\"\n");
    let mistake = encode_request_json(&schema, "read _", "[3]").unwrap_err();
    assert_eq!(mistake, "in `path`: found 3; expected a string");
}

#[test]
fn a_request_reads_back_as_json_calls() {
    let schema = poentaro();
    let request = encode_request_json(&schema, WRITE, r#"["a", 3, "x\ny"]"#).unwrap();
    assert_eq!(
        decode_request_json(&schema, &request).unwrap(),
        r#"[{"arguments":["a",3,"x\ny"],"method":"write _ at revision _ content _"}]"#
    );
    assert!(decode_request_json(&schema, "nonsense").is_err());
}

#[test]
fn a_response_is_written_from_json_outcomes() {
    let schema = poentaro();
    let request = encode_request_json(&schema, WRITE, r#"["a", 3, "x"]"#).unwrap();
    for outcome in [
        r#"{"kind":"value","value":4}"#,
        r#"{"error":{"content":"y","kind":"conflictAtRevisionContent","revision":4},"kind":"error"}"#,
        r#"{"kind":"internalError"}"#,
    ] {
        let response = encode_response_json(&schema, &request, &format!("[{outcome}]")).unwrap();
        assert_eq!(
            decode_response_json(&schema, WRITE, &response).unwrap(),
            outcome
        );
    }
    let mistake = encode_response_json(&schema, &request, r#"[{"kind":"value","value":"4"}]"#);
    assert_eq!(mistake.unwrap_err(), "found \"4\"; expected a whole number");
}

#[test]
fn each_outcome_reads_as_json() {
    let schema = poentaro();
    let cases = [
        (
            "list files",
            "list files RETURNS [{ path \"a\", revision 1 }]",
            r#"{"kind":"value","value":[{"path":"a","revision":1}]}"#,
        ),
        (
            WRITE,
            "write \"a\" at revision 3 content \"x\" ERROR conflict at revision 4 content \"y\"",
            r#"{"error":{"content":"y","kind":"conflictAtRevisionContent","revision":4},"kind":"error"}"#,
        ),
        (
            "read _",
            "read \"a\" ERROR trashed",
            r#"{"error":{"kind":"trashed"},"kind":"error"}"#,
        ),
        ("read _", "read \"a\" NOT RUN", r#"{"kind":"notRun"}"#),
        (
            "read _",
            "read \"a\" INTERNAL ERROR",
            r#"{"kind":"internalError"}"#,
        ),
        (
            "read _",
            "INVALID REQUEST { line 1, column 2, message \"m\" }",
            r#"{"column":2,"kind":"invalidRequest","line":1,"message":"m"}"#,
        ),
    ];
    for (method, response, json) in cases {
        assert_eq!(
            decode_response_json(&schema, method, response).unwrap(),
            json,
            "{response}"
        );
    }
}

#[test]
fn bytes_are_base64_in_json() {
    let schema =
        common::schema("PROTOCOL a VERSION 1, METHOD upload (file: BYTES) RETURNS (file: BYTES),");
    let request = encode_request_json(&schema, "upload _", r#"["TVRoZA=="]"#).unwrap();
    assert_eq!(request, "upload \"TVRoZA==\"\n");
    assert_eq!(
        decode_response_json(
            &schema,
            "upload _",
            "upload \"TVRoZA==\" RETURNS \"TVRoZA==\""
        )
        .unwrap(),
        r#"{"kind":"value","value":"TVRoZA=="}"#
    );
    let mistake = encode_request_json(&schema, "upload _", r#"["%%"]"#).unwrap_err();
    assert_eq!(
        mistake,
        "in `file`: found \"%%\"; expected base64 in a string"
    );
}

#[test]
fn nothing_returned_is_a_value_without_a_value_in_json() {
    let schema = common::schema("PROTOCOL a VERSION 1, METHOD sign out RETURNS NOTHING,");
    let json = decode_response_json(&schema, "sign out", "sign out RETURNS NOTHING").unwrap();
    assert_eq!(json, r#"{"kind":"value"}"#);
}
