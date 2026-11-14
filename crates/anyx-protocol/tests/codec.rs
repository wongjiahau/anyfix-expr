//! Requests and responses, both ways.

mod common;

use std::collections::BTreeMap;

use anyx_protocol::{
    Call, Outcome, Problem, Response, Schema, Value, decode_request, decode_response,
    encode_request, encode_response,
};
use common::{assert_snapshot, poentaro, schema};

fn method(schema: &Schema, shape: &str) -> usize {
    schema
        .methods
        .iter()
        .position(|method| method.shape.text == shape)
        .unwrap_or_else(|| panic!("no method `{shape}`"))
}

fn text(text: &str) -> Value {
    Value::Text(text.to_string())
}

fn record(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn variant(shape: &str, arguments: Vec<Value>) -> Value {
    Value::Variant {
        shape: shape.to_string(),
        arguments,
    }
}

const SCORE: &str = "title \"Ode\"\n\nmelody \"\"\"\n    1 2 3 -\n    \"\"\"\n";

const WRITE: &str = "write _ at revision _ content _";

#[test]
fn a_request_reads_back_as_it_was_written() {
    let schema = poentaro();
    let calls = vec![
        Call {
            method: method(&schema, "list files"),
            arguments: vec![],
        },
        Call {
            method: method(&schema, "read _"),
            arguments: vec![text("Hymns/a.poentaro")],
        },
        Call {
            method: method(&schema, WRITE),
            arguments: vec![text("Hymns/a.poentaro"), Value::Integer(-3), text(SCORE)],
        },
    ];
    let request = encode_request(&schema, &calls).unwrap();
    assert_snapshot("tests/snapshots/request.anyx", &request);
    assert_eq!(decode_request(&schema, &request).unwrap(), calls);
}

#[test]
fn a_response_reads_back_as_it_was_written() {
    let schema = poentaro();
    let read = |path: &str| Call {
        method: method(&schema, "read _"),
        arguments: vec![text(path)],
    };
    let write = Call {
        method: method(&schema, WRITE),
        arguments: vec![text("Hymns/a.poentaro"), Value::Integer(3), text(SCORE)],
    };
    let calls = [
        Call {
            method: method(&schema, "list files"),
            arguments: vec![],
        },
        read("a.poentaro"),
        write.clone(),
        write,
        read("a.poentaro"),
        read("b.poentaro"),
        read("c.poentaro"),
    ];
    let methods: Vec<usize> = calls.iter().map(|call| call.method).collect();
    let summary = |path: &str, revision| {
        record(&[("path", text(path)), ("revision", Value::Integer(revision))])
    };
    let diagnostic = record(&[
        ("line", Value::Integer(2)),
        ("column", Value::Integer(5)),
        ("code", text("E1")),
        ("message", text("found \"x\"; expected a note")),
    ]);
    let response = Response::Outcomes(vec![
        Outcome::Value(Value::List(vec![
            summary("a.poentaro", 1),
            summary("b/c.poentaro", 7),
        ])),
        Outcome::Value(record(&[
            ("path", text("a.poentaro")),
            ("revision", Value::Integer(1)),
            ("content", text(SCORE)),
        ])),
        Outcome::Error(variant(
            "conflict at revision _ content _",
            vec![Value::Integer(4), text(SCORE)],
        )),
        Outcome::Error(variant("rejected _", vec![Value::List(vec![diagnostic])])),
        Outcome::Error(variant("trashed", vec![])),
        Outcome::InternalError,
        Outcome::NotRun,
    ]);
    let text = encode_response(&schema, &calls, &response).unwrap();
    assert_snapshot("tests/snapshots/response.anyx", &text);
    assert_eq!(decode_response(&schema, &methods, &text).unwrap(), response);
}

#[test]
fn an_error_must_be_one_of_the_methods() {
    let schema = poentaro();
    let response = Response::Outcomes(vec![Outcome::Error(variant(
        "invalid path _",
        vec![text("x")],
    ))]);
    let read = Call {
        method: method(&schema, "read _"),
        arguments: vec![text("a.poentaro")],
    };
    let error = encode_response(&schema, &[read], &response).unwrap_err();
    assert_eq!(
        error.message,
        "found the error `invalid path _`; expected an error of `read _`"
    );
}

#[test]
fn an_invalid_request_reads_back() {
    let schema = poentaro();
    let response = Response::InvalidRequest(Problem {
        line: 2,
        column: 4,
        message: "found `x`; expected \"y\"".to_string(),
    });
    let text = encode_response(&schema, &[], &response).unwrap();
    assert_eq!(
        text,
        "INVALID REQUEST { line 2, column 4, message \"found `x`; expected \\\"y\\\"\" }\n"
    );
    assert_eq!(decode_response(&schema, &[0, 1], &text).unwrap(), response);
}

const MODES: &str = "PROTOCOL a VERSION 1,
CHOICE shade { dark, light },
CHOICE mode { dark, light, custom hue (hue: NUMBER), (base: shade) dimmed },
RECORD options { mode: mode, strict: OPTIONAL: BOOLEAN },
METHOD set (mode: mode) strict (strict: BOOLEAN) RETURNS (ok: BOOLEAN),
METHOD apply (options: options) RETURNS (modes: LIST: mode),";

#[test]
fn words_in_slots_go_in_parentheses() {
    let schema = schema(MODES);
    let custom = variant("custom hue _", vec![Value::Number(0.5)]);
    let calls = vec![
        Call {
            method: 0,
            arguments: vec![variant("dark", vec![]), Value::Boolean(true)],
        },
        Call {
            method: 0,
            arguments: vec![custom.clone(), Value::Boolean(false)],
        },
        Call {
            method: 0,
            arguments: vec![
                variant("_ dimmed", vec![variant("dark", vec![])]),
                Value::Boolean(false),
            ],
        },
        Call {
            method: 1,
            arguments: vec![record(&[(
                "mode",
                variant("_ dimmed", vec![variant("light", vec![])]),
            )])],
        },
    ];
    let request = encode_request(&schema, &calls).unwrap();
    assert_eq!(
        request,
        "set (dark) strict (true),
set (custom hue 0.5) strict (false),
set ((dark) dimmed) strict (false),
apply { mode: (light) dimmed },
"
    );
    assert_eq!(decode_request(&schema, &request).unwrap(), calls);
}

#[test]
fn text_that_a_raw_string_cannot_hold_is_quoted() {
    let schema = poentaro();
    for content in ["a\n   \nb", "\"\"\"\nx", "x\r\ny", "tab\there\n"] {
        let calls = vec![Call {
            method: method(&schema, "create _ content _"),
            arguments: vec![text("a"), text(content)],
        }];
        let request = encode_request(&schema, &calls).unwrap();
        assert_eq!(
            decode_request(&schema, &request).unwrap(),
            calls,
            "{request}"
        );
    }
}

#[test]
fn a_number_that_is_not_finite_has_no_form() {
    let schema = schema("PROTOCOL a VERSION 1, METHOD f (x: NUMBER) RETURNS (y: NUMBER),");
    let calls = vec![Call {
        method: 0,
        arguments: vec![Value::Number(f64::NAN)],
    }];
    assert!(encode_request(&schema, &calls).is_err());
}

fn render(cases: &[&str], decode: impl Fn(&str) -> Result<String, Problem>) -> String {
    cases
        .iter()
        .map(|source| match decode(source) {
            Ok(read) => panic!("{source:?} read as {read}"),
            Err(problem) => format!("{source}\n=> {problem}\n"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn mistakes_in_a_request_point_at_where_they_are() {
    let schema = poentaro();
    let cases = [
        "",
        "reed \"a\"",
        "read 3",
        "write \"a\" at revision 1.5 content \"x\"",
        "write \"a\" at revision 99999999999999999999 content \"x\"",
        "read \"a\",\n\"loose\"",
        "read \"a",
        "list files, read [\"a\"]",
    ];
    assert_snapshot(
        "tests/snapshots/request_errors.txt",
        &render(&cases, |source| {
            decode_request(&schema, source).map(|calls| format!("{calls:?}"))
        }),
    );
}

#[test]
fn mistakes_in_values_name_the_parameter_and_field() {
    let schema = schema(MODES);
    let cases = [
        "apply { mode: dim }",
        "apply { strict: true }",
        "apply { mode: dark, mode: dark }",
        "apply { mode: dark, colour: dark }",
        "apply { mode: custom hue }",
        "apply { mode: dark hue 1 }",
        "apply { mode: custom hue \"red\" }",
        "apply { mode dark }",
        "apply { mode: dark, strict: yes }",
        "set dark strict (true)",
    ];
    assert_snapshot(
        "tests/snapshots/value_errors.txt",
        &render(&cases, |source| {
            decode_request(&schema, source).map(|calls| format!("{calls:?}"))
        }),
    );
}

#[test]
fn mistakes_in_a_response_point_at_where_they_are() {
    let schema = poentaro();
    let read = [method(&schema, "read _")];
    let cases = [
        "read \"a\" ERROR conflict at revision 1 content \"x\"",
        "read \"a\" ERROR gone",
        "read \"a\" RETURNS missing",
        "read \"a\" NOT RUN, read \"b\" NOT RUN",
        "INVALID REQUEST { line 1 }",
        "ERROR missing",
        "list files RETURNS []",
        "read \"a\" ERRORS missing",
    ];
    assert_snapshot(
        "tests/snapshots/response_errors.txt",
        &render(&cases, |source| {
            decode_response(&schema, &read, source).map(|response| format!("{response:?}"))
        }),
    );
}

const TAGS: &str = "PROTOCOL a VERSION 1,
METHOD tag (names: LIST: TEXT) note (note: TEXT) RETURNS (count: INTEGER),";

#[test]
fn a_response_repeats_each_call_with_long_arguments_shortened() {
    let schema = schema(TAGS);
    let names = |count: usize| Value::List((1..=count).map(|n| text(&format!("n{n}"))).collect());
    let calls = [
        Call {
            method: 0,
            arguments: vec![names(3), text("short\nnote")],
        },
        Call {
            method: 0,
            arguments: vec![names(4), text(&"a".repeat(33))],
        },
        Call {
            method: 0,
            arguments: vec![names(0), text(&"é".repeat(32))],
        },
    ];
    let response = Response::Outcomes(vec![
        Outcome::Value(Value::Integer(3)),
        Outcome::InternalError,
        Outcome::NotRun,
    ]);
    let methods = [0, 0, 0];
    let written = encode_response(&schema, &calls, &response).unwrap();
    assert_eq!(
        written,
        format!(
            "tag [\"n1\", \"n2\", \"n3\"] note \"short\\nnote\" RETURNS 3,\n\
             tag [\"n1\", \"n2\", \"n3\", …] note \"{}…\" INTERNAL ERROR,\n\
             tag [] note \"{}\" NOT RUN,\n",
            "a".repeat(32),
            "é".repeat(32)
        )
    );
    assert_eq!(
        decode_response(&schema, &methods, &written).unwrap(),
        response
    );
}

#[test]
fn a_method_that_returns_nothing_answers_returns_nothing() {
    let schema = schema(
        "PROTOCOL a VERSION 1,
        METHOD sign out RETURNS NOTHING,
        METHOD count RETURNS (count: INTEGER),",
    );
    let sign_out = method(&schema, "sign out");
    let count = method(&schema, "count");
    let calls = [
        Call {
            method: sign_out,
            arguments: vec![],
        },
        Call {
            method: count,
            arguments: vec![],
        },
    ];
    let response = Response::Outcomes(vec![
        Outcome::Value(Value::Nothing),
        Outcome::Value(Value::Integer(2)),
    ]);
    let written = encode_response(&schema, &calls, &response).unwrap();
    assert_eq!(written, "sign out RETURNS NOTHING,\ncount RETURNS 2,\n");
    assert_eq!(
        decode_response(&schema, &[sign_out, count], &written).unwrap(),
        response
    );
}

#[test]
fn nothing_and_a_value_are_not_mixed_up() {
    let schema = schema(
        "PROTOCOL a VERSION 1,
        METHOD sign out RETURNS NOTHING,
        METHOD count RETURNS (count: INTEGER),",
    );
    let sign_out = method(&schema, "sign out");
    let count = method(&schema, "count");
    assert!(
        decode_response(&schema, &[sign_out], "sign out RETURNS 2")
            .unwrap_err()
            .message
            .contains("expected `RETURNS NOTHING`")
    );
    assert!(
        decode_response(&schema, &[count], "count RETURNS NOTHING")
            .unwrap_err()
            .message
            .contains("found `RETURNS NOTHING`")
    );
    let call = Call {
        method: sign_out,
        arguments: vec![],
    };
    assert!(
        encode_response(
            &schema,
            &[call],
            &Response::Outcomes(vec![Outcome::Value(Value::Integer(1))])
        )
        .is_err()
    );
}
