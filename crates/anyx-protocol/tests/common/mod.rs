#![allow(dead_code)]

use anyx_protocol::Schema;

/// The file at `path`, relative to this crate.
pub fn read(path: &str) -> String {
    let path = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// Compares `actual` with the file at `path`. Run the tests with
/// `UPDATE_SNAPSHOTS=1` to write the file instead.
pub fn assert_snapshot(path: &str, actual: &str) {
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

pub fn poentaro() -> Schema {
    schema(&read("tests/fixtures/poentaro.protocol.anyx"))
}

pub fn schema(source: &str) -> Schema {
    Schema::parse(source).unwrap_or_else(|diagnostics| {
        let rendered: Vec<String> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.render(source, "schema"))
            .collect();
        panic!("{}", rendered.join("\n"))
    })
}

/// Each diagnostic for `source`, rendered.
pub fn diagnostics(source: &str) -> String {
    match Schema::parse(source) {
        Ok(_) => panic!("expected diagnostics for {source:?}"),
        Err(diagnostics) => diagnostics
            .iter()
            .map(|diagnostic| diagnostic.render(source, "schema"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
