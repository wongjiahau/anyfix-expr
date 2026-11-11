//! The `anyfix-generate` command.

mod common;

use std::process::{Command, Output};

use common::fixture;

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_anyfix-generate"))
        .args(arguments)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("the command runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("the command prints text")
}

/// Writes a grammar to a file of its own and gives its path.
fn grammar_file(name: &str, source: &str) -> String {
    let directory = std::env::temp_dir().join(format!("anyfix-generate-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(name);
    std::fs::write(&path, source).unwrap();
    path.to_str().expect("a path of text").to_string()
}

#[test]
fn it_prints_the_parser_laid_out() {
    let output = run(&["tests/fixtures/json.anyfix"]);
    assert!(output.status.success());
    assert_eq!(text(&output.stderr), "");
    assert!(text(&output.stdout) == fixture("tests/generated/json.rs"));
}

#[test]
fn unformatted_output_is_the_same_code() {
    let output = run(&["--unformatted", "tests/fixtures/json.anyfix"]);
    assert!(output.status.success());
    let formatted = anyfix_generator::format(&text(&output.stdout)).unwrap();
    assert!(formatted == fixture("tests/generated/json.rs"));
}

#[test]
fn errors_show_the_line_and_stop_the_output() {
    let path = grammar_file(
        "unknown.anyfix",
        "ENTRY: value,\nvalue: ONE OF [text: STRING, lsit],\nlist: SQUARE BRACKET: ZERO OR MORE: value",
    );
    let output = run(&[&path]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stdout), "");
    assert_eq!(
        text(&output.stderr),
        format!(
            "{path}:2:30: error: found a reference to `lsit`, which is not a rule; expected the name of a rule of this grammar; write `list`, or define `lsit: ...`
  value: ONE OF [text: STRING, lsit],
                               ^^^^

"
        )
    );
}

#[test]
fn warnings_are_shown_and_the_parser_is_still_printed() {
    let path = grammar_file(
        "competing.anyfix",
        "ENTRY: value,\nvalue: ONE OF [any: STRING, empty: STRING \"\"]",
    );
    let output = run(&[&path]);
    assert!(output.status.success());
    assert!(text(&output.stdout).contains("pub enum Value {"));
    assert_eq!(
        text(&output.stderr),
        format!(
            "{path}:2:29: warning: found the choices `any` and `empty` able to match the same input; expected at most one choice to fit any input, since `any` comes first and always wins it; make one of them more specific, or put the more specific one first
  value: ONE OF [any: STRING, empty: STRING \"\"]
                              ^^^^^^^^^^^^^^^^

"
        )
    );
}

#[test]
fn a_missing_file_or_argument_is_explained() {
    let output = run(&["no-such-file.anyfix"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).starts_with("no-such-file.anyfix: "));

    let output = run(&[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).starts_with("found no grammar file; expected its path"));
}
