//! Values as JSON, for the TypeScript client.

mod common;

use anyx_protocol::json::{decode_response_json, encode_request_json};
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
fn each_outcome_reads_as_json() {
    let schema = poentaro();
    let cases = [
        (
            "list files",
            "[{ path \"a\", revision 1 }]",
            r#"{"kind":"value","value":[{"path":"a","revision":1}]}"#,
        ),
        (
            WRITE,
            "ERROR conflict at revision 4 content \"y\"",
            r#"{"error":{"content":"y","kind":"conflictAtRevisionContent","revision":4},"kind":"error"}"#,
        ),
        (
            "read _",
            "ERROR trashed",
            r#"{"error":{"kind":"trashed"},"kind":"error"}"#,
        ),
        ("read _", "NOT RUN", r#"{"kind":"notRun"}"#),
        ("read _", "INTERNAL ERROR", r#"{"kind":"internalError"}"#),
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
