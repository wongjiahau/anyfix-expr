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

/// The files of the schema in `tests/fixtures/<folder>`, each path relative
/// to it, in no particular order.
pub fn schema_files(folder: &str) -> Vec<(String, String)> {
    fn walk(root: &std::path::Path, at: &std::path::Path, files: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, files);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((relative, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let root = std::path::PathBuf::from(format!(
        "{}/tests/fixtures/{folder}",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut files = Vec::new();
    walk(&root, &root, &mut files);
    files
}

/// Reads the schema of `files`, each a path and its source.
pub fn parse_files(files: &[(&str, &str)]) -> Result<Schema, Vec<anyx_protocol::Diagnostic>> {
    let files: Vec<anyx_protocol::SchemaFile> = files
        .iter()
        .map(|(path, source)| anyx_protocol::SchemaFile { path, source })
        .collect();
    Schema::parse_files(&files)
}

/// The scoped schema in `tests/fixtures/scoped`.
pub fn scoped() -> Schema {
    let files = schema_files("scoped");
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, source)| (path.as_str(), source.as_str()))
        .collect();
    parse_files(&files).unwrap_or_else(|diagnostics| panic!("{}", render(&files, &diagnostics)))
}

/// Each diagnostic for the schema of `files`, rendered in its file.
pub fn render(files: &[(&str, &str)], diagnostics: &[anyx_protocol::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let source = files
                .iter()
                .find(|(path, _)| *path == diagnostic.path)
                .map_or("", |(_, source)| source);
            diagnostic.render(source, &diagnostic.path)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
