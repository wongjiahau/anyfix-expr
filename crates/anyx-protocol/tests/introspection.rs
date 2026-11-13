//! `SCHEMA`, the call every server answers with the guide to its schema.

mod common;

use anyx_protocol::server::introspect;
use anyx_protocol::{
    Call, Outcome, Response, Value, decode_request, decode_response, encode_request,
    encode_response,
};
use common::scoped;

#[test]
fn schema_is_a_call_of_every_schema() {
    let schema = scoped();
    let introspection = schema.introspection();
    assert_eq!(introspection, schema.methods.len());
    assert_eq!(schema.method_at(introspection).full_name(), "SCHEMA");
    let calls = decode_request(&schema, "SCHEMA, files/list").unwrap();
    assert_eq!(
        calls,
        [
            Call {
                method: introspection,
                arguments: vec![],
            },
            Call {
                method: 0,
                arguments: vec![],
            },
        ]
    );
    assert_eq!(
        encode_request(&schema, &calls).unwrap(),
        "SCHEMA,\nfiles/list,\n"
    );
}

#[test]
fn schema_answers_the_guide_without_how_to_send() {
    let schema = scoped();
    let Outcome::Value(Value::Text(guide)) = introspect(&schema) else {
        panic!("expected the guide as text");
    };
    assert_eq!(guide, anyx_protocol::guide(&schema, ""));
    assert!(
        guide.starts_with("# notes (anyx protocol, version 1)\n\n## Requests\n"),
        "{guide}"
    );
    assert!(guide.contains("`SCHEMA` is a call every server answers"));
    let methods = [schema.introspection()];
    let call = Call {
        method: schema.introspection(),
        arguments: vec![],
    };
    let response = encode_response(
        &schema,
        &[call],
        &Response::Outcomes(vec![introspect(&schema)]),
    )
    .unwrap();
    assert_eq!(
        decode_response(&schema, &methods, &response).unwrap(),
        Response::Outcomes(vec![Outcome::Value(Value::Text(guide))])
    );
}

#[test]
fn schema_is_not_in_a_scope() {
    let message = decode_request(&scoped(), "files/SCHEMA")
        .unwrap_err()
        .to_string();
    assert!(message.contains("expected one of the methods"), "{message}");
}

#[cfg(feature = "json")]
#[test]
fn the_json_client_calls_schema() {
    let schema = scoped();
    assert_eq!(
        anyx_protocol::json::encode_request_json(&schema, "SCHEMA", "[]").unwrap(),
        "SCHEMA\n"
    );
    assert_eq!(
        anyx_protocol::json::decode_response_json(&schema, "SCHEMA", "SCHEMA RETURNS \"guide\"")
            .unwrap(),
        r#"{"kind":"value","value":"guide"}"#
    );
}
