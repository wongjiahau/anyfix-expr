//! A schema of a folder: a scope per file, methods called by their full
//! names, and types and errors private to their scope unless `PUBLIC`.

mod common;

use anyx_protocol::{Call, Value, decode_request, encode_request};
use common::{assert_snapshot, parse_files, render, scoped};

#[test]
fn methods_are_named_by_their_scope() {
    let schema = scoped();
    let names: Vec<String> = schema
        .methods
        .iter()
        .map(|method| method.full_name())
        .collect();
    assert_eq!(
        names,
        [
            "files/list",
            "files/read _",
            "files/write _ at revision _ content _",
            "files/export/html _",
            "files/export/pdf _",
        ]
    );
    let scopes: Vec<String> = schema
        .scopes()
        .iter()
        .map(|scope| scope.join("/"))
        .collect();
    assert_eq!(scopes, ["", "common", "files", "files/export"]);
    let write = schema
        .method("files/write _ at revision _ content _")
        .unwrap();
    assert_eq!(
        schema.type_name(&write.returns.r#type),
        "LIST: common/diagnostic"
    );
    let errors: Vec<&str> = schema
        .errors_of(schema.method("files/export/pdf _").unwrap())
        .iter()
        .map(|error| error.shape.text.as_str())
        .collect();
    assert_eq!(errors, ["unreadable _", "trashed", "missing"]);
    assert_eq!(schema.name, "notes");
    assert_eq!(schema.files[0].path, "protocol.anyx");
}

#[test]
fn a_call_in_a_scope_reads_back_as_it_was_written() {
    let schema = scoped();
    let pdf = schema
        .methods
        .iter()
        .position(|method| method.full_name() == "files/export/pdf _")
        .unwrap();
    let list = schema
        .methods
        .iter()
        .position(|method| method.full_name() == "files/list")
        .unwrap();
    let calls = vec![
        Call {
            method: pdf,
            arguments: vec![Value::Text("a.note".to_string())],
        },
        Call {
            method: list,
            arguments: vec![],
        },
    ];
    let request = encode_request(&schema, &calls).unwrap();
    assert_eq!(request, "files/export/pdf \"a.note\",\nfiles/list,\n");
    assert_eq!(decode_request(&schema, &request).unwrap(), calls);
    assert_eq!(
        decode_request(&schema, "files / export / pdf \"a.note\", files/list").unwrap(),
        calls
    );
}

fn request_error(request: &str) -> String {
    decode_request(&scoped(), request).unwrap_err().to_string()
}

#[test]
fn a_call_outside_its_scope_is_pointed_to_its_full_name() {
    assert_eq!(
        request_error("read \"a\""),
        "1:1: found the call `read _`; expected one of the methods `files/list`, `files/read _`, `files/write _ at revision _ content _`, `files/export/html _`, `files/export/pdf _`; a word in a slot, such as a choice's variant or `true`, goes in parentheses; a method of that shape is in a scope, so write its full name: `files/read _`"
    );
    assert_eq!(
        request_error("files/pdf \"a\""),
        "1:1: found the call `files/pdf _`; expected one of the methods `files/list`, `files/read _`, `files/write _ at revision _ content _`, `files/export/html _`, `files/export/pdf _`; a word in a slot, such as a choice's variant or `true`, goes in parentheses; a method of that shape is in a scope, so write its full name: `files/export/pdf _`"
    );
    assert_eq!(
        request_error("common/read \"a\""),
        "1:1: found the scope `common`, which has no methods; expected one of the methods `files/list`, `files/read _`, `files/write _ at revision _ content _`, `files/export/html _`, `files/export/pdf _`"
    );
}

#[cfg(feature = "json")]
#[test]
fn the_json_client_calls_a_method_by_its_full_name() {
    let schema = scoped();
    assert_eq!(
        anyx_protocol::json::encode_request_json(&schema, "files/export/pdf _", "[\"a\"]").unwrap(),
        "files/export/pdf \"a\"\n"
    );
    assert_eq!(
        anyx_protocol::json::decode_response_json(&schema, "files/export/pdf _", "ERROR missing")
            .unwrap(),
        r#"{"error":{"kind":"missing"},"kind":"error"}"#
    );
}

#[test]
fn the_guide_shows_each_scope_under_its_full_name() {
    let guide = anyx_protocol::guide(&scoped(), "Send a request as the body of `POST /notes`.");
    assert_snapshot("tests/snapshots/scoped_guide.md", &guide);
}

const ROOT: (&str, &str) = ("protocol.anyx", "PROTOCOL notes VERSION 1,");

const COMMON: (&str, &str) = (
    "common.anyx",
    "PUBLIC RECORD diagnostic { line: INTEGER }, RECORD secret { a: TEXT }, ERROR gone, PUBLIC ERROR missing,",
);

/// Each case: a title and the schema's files.
const CASES: &[(&str, &[(&str, &str)])] = &[
    (
        "a private type from another scope",
        &[
            ROOT,
            COMMON,
            ("files.anyx", "RECORD file { a: common/secret },"),
        ],
    ),
    (
        "a full path that names nothing",
        &[
            ROOT,
            COMMON,
            ("files.anyx", "RECORD file { a: common/nothing },"),
        ],
    ),
    (
        "a public type written without its path",
        &[
            ROOT,
            COMMON,
            ("files.anyx", "RECORD file { a: diagnostic },"),
        ],
    ),
    (
        "a private error from another scope",
        &[
            ROOT,
            COMMON,
            (
                "files.anyx",
                "METHOD read RETURNS (a: TEXT) ERRORS common/gone,",
            ),
        ],
    ),
    (
        "a public error written without its path",
        &[
            ROOT,
            COMMON,
            (
                "files.anyx",
                "METHOD read RETURNS (a: TEXT) ERRORS missing,",
            ),
        ],
    ),
    (
        "errors of the same words from two scopes",
        &[
            ROOT,
            COMMON,
            (
                "files.anyx",
                "ERROR missing, METHOD read RETURNS (a: TEXT) ERRORS [missing, common/missing],",
            ),
        ],
    ),
    (
        "a path of two words",
        &[
            ROOT,
            COMMON,
            ("files.anyx", "RECORD file { a: my common/diagnostic },"),
        ],
    ),
    (
        "a definition next to the header",
        &[
            (
                "protocol.anyx",
                "PROTOCOL notes VERSION 1, RECORD x { a: TEXT },",
            ),
            COMMON,
        ],
    ),
    (
        "a header in a scope",
        &[ROOT, ("files.anyx", "PROTOCOL files VERSION 1,")],
    ),
    ("no root file", &[("files.anyx", "RECORD x { a: TEXT },")]),
    (
        "a file name that is no scope",
        &[ROOT, ("Files.anyx", "RECORD x { a: TEXT },")],
    ),
    (
        "a file that is not anyx",
        &[ROOT, ("files.txt", "RECORD x { a: TEXT },")],
    ),
    (
        "a `/` in a method's name",
        &[
            ROOT,
            (
                "files.anyx",
                "METHOD files/read (path: TEXT) RETURNS (a: TEXT),",
            ),
        ],
    ),
];

#[test]
fn mistakes_in_scopes_are_diagnosed() {
    let rendered: String = CASES
        .iter()
        .map(|(title, files)| {
            let diagnostics = match parse_files(files) {
                Ok(_) => panic!("expected diagnostics for {title}"),
                Err(diagnostics) => diagnostics,
            };
            format!("== {title}\n{}\n\n", render(files, &diagnostics))
        })
        .collect();
    assert_snapshot("tests/snapshots/scope_diagnostics.txt", &rendered);
}
