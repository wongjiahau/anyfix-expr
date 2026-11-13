//! The code generated from Poentaro's schema, and from the schema of scopes
//! in `anyx-protocol`'s fixtures, kept in `tests/generated` so a change to
//! the generator shows up as a diff. `round_trip.rs` and
//! `scoped_round_trip.rs` compile the Rust modules.

use anyx_protocol::{Schema, SchemaFile};

fn read(path: &str) -> String {
    let full = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&full).unwrap_or_else(|error| panic!("{full}: {error}"))
}

fn assert_snapshot(path: &str, actual: &str) {
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        let full = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::write(&full, actual).unwrap_or_else(|error| panic!("{full}: {error}"));
    } else {
        assert!(
            read(path) == actual,
            "{path} is out of date; run with UPDATE_SNAPSHOTS=1 and review the diff"
        );
    }
}

fn poentaro() -> Schema {
    Schema::parse(&read(
        "../anyx-protocol/tests/fixtures/poentaro.protocol.anyx",
    ))
    .unwrap()
}

/// The scoped fixture: a file per scope.
fn scoped() -> Schema {
    let paths = [
        "protocol.anyx",
        "common.anyx",
        "files.anyx",
        "files/export.anyx",
    ];
    let sources: Vec<String> = paths
        .iter()
        .map(|path| read(&format!("../anyx-protocol/tests/fixtures/scoped/{path}")))
        .collect();
    let files: Vec<SchemaFile> = paths
        .iter()
        .zip(&sources)
        .map(|(path, source)| SchemaFile { path, source })
        .collect();
    Schema::parse_files(&files).unwrap()
}

#[test]
fn the_scoped_rust_module_is_up_to_date() {
    let code = anyx_protocol_generate::rust(&scoped()).unwrap();
    let code = anyx_protocol_generate::format_rust(&code).expect("rustfmt is installed");
    assert_snapshot("tests/generated/scoped.rs", &code);
}

#[test]
fn the_scoped_typescript_client_is_up_to_date() {
    let code = anyx_protocol_generate::typescript(&scoped()).unwrap();
    assert_snapshot("tests/generated/scoped.ts", &code);
}

#[test]
fn the_rust_module_is_up_to_date() {
    let code = anyx_protocol_generate::rust(&poentaro()).unwrap();
    let code = anyx_protocol_generate::format_rust(&code).expect("rustfmt is installed");
    assert_snapshot("tests/generated/poentaro.rs", &code);
}

#[test]
fn the_wasm_bindings_are_up_to_date() {
    let code = anyx_protocol_generate::wasm(&poentaro());
    let code = anyx_protocol_generate::format_rust(&code).expect("rustfmt is installed");
    assert_snapshot("tests/generated/poentaro_wasm.rs", &code);
}

#[test]
fn the_typescript_client_is_up_to_date() {
    let code = anyx_protocol_generate::typescript(&poentaro()).unwrap();
    assert_snapshot("tests/generated/poentaro.ts", &code);
}

fn error(source: &str) -> String {
    let schema = Schema::parse(source).unwrap();
    let rust = anyx_protocol_generate::rust(&schema).unwrap_err();
    let typescript = anyx_protocol_generate::typescript(&schema).unwrap_err();
    assert_eq!(rust, typescript);
    rust.message
}

#[test]
fn names_that_become_the_same_name_are_rejected() {
    assert_eq!(
        error(
            "PROTOCOL p VERSION 1, RECORD read error { a: TEXT }, ERROR gone, METHOD read (path: TEXT) RETURNS (text: TEXT) ERRORS [gone]"
        ),
        "found the errors of `read _`, which becomes the type `ReadError`, like the type `read error`; expected names that stay distinct in the generated code, so rename one"
    );
    assert_eq!(
        error("PROTOCOL p VERSION 1, RECORD client { a: TEXT }"),
        "found the type `client`, which becomes the type `Client`, like the generated `Client`; expected names that stay distinct in the generated code, so rename one"
    );
    assert_eq!(
        error("PROTOCOL p VERSION 1, METHOD send (text: TEXT) RETURNS (sent: BOOLEAN)"),
        "found `send _`, which becomes the method `send`, like the client's `send`; expected names that stay distinct in the generated code, so rename one"
    );
}

#[test]
fn a_keyword_becomes_a_raw_identifier() {
    let schema =
        Schema::parse("PROTOCOL p VERSION 1, RECORD box { type: TEXT, loop: INTEGER }").unwrap();
    let code = anyx_protocol_generate::rust(&schema).unwrap();
    assert!(code.contains("pub r#type: String"), "{code}");
    assert!(code.contains("pub r#loop: i64"), "{code}");
}

fn scoped_error(files: &[(&str, &str)]) -> String {
    let files: Vec<SchemaFile> = files
        .iter()
        .map(|(path, source)| SchemaFile { path, source })
        .collect();
    let schema = Schema::parse_files(&files).unwrap();
    let rust = anyx_protocol_generate::rust(&schema).unwrap_err();
    let typescript = anyx_protocol_generate::typescript(&schema).unwrap_err();
    assert_eq!(rust, typescript);
    rust.message
}

#[test]
fn names_in_scopes_that_become_the_same_name_are_rejected() {
    let root = ("protocol.anyx", "PROTOCOL p VERSION 1,");
    assert_eq!(
        scoped_error(&[
            root,
            ("files.anyx", "METHOD read RETURNS (a: TEXT),"),
            ("files/read.anyx", "METHOD open RETURNS (a: TEXT),"),
        ]),
        "found the scope `files/read` in the scope `files`, which becomes the method `read`, like `files/read`; expected names that stay distinct in the generated code, so rename one"
    );
    assert_eq!(
        scoped_error(&[
            root,
            ("files.anyx", "RECORD export page { a: TEXT },"),
            ("files/export.anyx", "RECORD page { a: TEXT },"),
        ]),
        "found the type `files/export/page`, which becomes the TypeScript type `FilesExportPage`, like the type `files/export page`; expected names that stay distinct in the generated code, so rename one"
    );
}
