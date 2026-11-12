#![allow(dead_code)]

/// Compares `actual` with `tests/snapshots/<name>`. Run the tests with
/// `UPDATE_SNAPSHOTS=1` to write the file instead, then review its diff.
pub fn assert_snapshot(name: &str, actual: &str) {
    let path = format!("{}/tests/snapshots/{name}", env!("CARGO_MANIFEST_DIR"));
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, actual).unwrap_or_else(|error| panic!("{path}: {error}"));
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        expected == actual,
        "{name} differs from its snapshot; run with UPDATE_SNAPSHOTS=1 and review the diff.\n--- actual ---\n{actual}"
    );
}

pub fn fixture(path: &str) -> String {
    let path = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}
