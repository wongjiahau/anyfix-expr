//! Raw strings: three or more quotes, content on the lines between, the
//! closing line's indentation stripped, and no escapes.

mod common;

use anyx::{NodeKind, StringLiteral};
use common::{nodes, tree};

/// The literal of a source that is one string, or one call of one string.
fn literal(source: &str) -> StringLiteral {
    let nodes = nodes(source);
    let node = match &nodes[..] {
        [node] => node,
        other => panic!("expected one node in {source:?}, got {other:?}"),
    };
    match &node.kind {
        NodeKind::String(literal) => literal.clone(),
        NodeKind::Call(call) => match &call.arguments[..] {
            [argument] => match &argument.kind {
                NodeKind::String(literal) => literal.clone(),
                other => panic!("expected a string, got {other:?}"),
            },
            other => panic!("expected one argument, got {other:?}"),
        },
        other => panic!("expected a string, got {other:?}"),
    }
}

fn value(source: &str) -> String {
    literal(source).value
}

#[test]
fn the_closing_lines_indentation_is_stripped() {
    let source = "score \"\"\"\n    a b\n      c\n    \"\"\"";
    assert_eq!(value(source), "a b\n  c");
    assert_eq!(tree(source), r#"(`score _` "a b\n  c")"#);
}

#[test]
fn no_escapes_and_quotes_are_content() {
    let source = "\"\"\"\n  label=\"Verse\" \\n \\q #not a comment\n  a \"\"\" b\n  \"\"\"";
    assert_eq!(
        value(source),
        "label=\"Verse\" \\n \\q #not a comment\na \"\"\" b"
    );
}

#[test]
fn more_quotes_let_the_content_hold_three() {
    let source = "\"\"\"\"\n  \"\"\"\n  \"\"\"\"";
    assert_eq!(value(source), "\"\"\"");
    let five = "\"\"\"\n\"\"\"\"\"\n\"\"\"";
    assert_eq!(value(five), "\"\"\"\"\"");
}

#[test]
fn blank_lines_and_empty_strings() {
    assert_eq!(value("\"\"\"\n  a\n\n   \n  b\n  \"\"\""), "a\n\n\nb");
    assert_eq!(value("\"\"\"\n\"\"\""), "");
    assert_eq!(value("\"\"\"\n\n\"\"\""), "");
    assert_eq!(value("\"\"\"\n  \"\"\""), "");
}

#[test]
fn trailing_whitespace_after_the_opening_quotes_is_fine() {
    assert_eq!(value("\"\"\"  \t\n  a\n  \"\"\""), "a");
    assert_eq!(value("\"\"\"\r\n  a\r\n  b\r\n  \"\"\""), "a\r\nb");
}

#[test]
fn tokens_continue_after_the_closing_quotes() {
    let source = "a \"\"\"\n  x\n  \"\"\", b # done\n";
    assert_eq!(tree(source), r#"(`a _` "x"), b"#);
    assert_eq!(
        tree("f : \"\"\"\n  x\n  \"\"\" . g"),
        r#"(`f _` (`_ g` "x"))"#
    );
}

#[test]
fn two_quotes_are_still_an_empty_string() {
    assert_eq!(tree(r#""""#), r#""""#);
    assert_eq!(tree(r#""" """#), r#"(`_ _` "" "")"#);
}

#[test]
fn a_value_offset_maps_back_to_the_source() {
    let source = "score \"\"\"\n    bpm=90\n\n    [S1] 3 3\n    \"\"\"";
    let literal = literal(source);
    assert_eq!(literal.value, "bpm=90\n\n[S1] 3 3");
    for needle in ["bpm", "90", "[S1]", "3 3"] {
        let offset = literal.source_offset(literal.value.find(needle).unwrap());
        assert_eq!(&source[offset..offset + needle.len()], needle);
    }
    let end = literal.source_offset(literal.value.len());
    assert_eq!(&source[..end], "score \"\"\"\n    bpm=90\n\n    [S1] 3 3");
}

#[test]
fn a_value_offset_inside_an_escape_maps_to_the_escape() {
    let source = r#""a\u00e9b\nc""#;
    let literal = literal(source);
    assert_eq!(literal.value, "aéb\nc");
    assert_eq!(literal.source_offset(0), 1);
    assert_eq!(literal.source_offset(1), 2);
    assert_eq!(literal.source_offset(2), 2);
    assert_eq!(literal.source_offset(3), 8);
    assert_eq!(literal.source_offset(4), 9);
    assert_eq!(literal.source_offset(5), 11);
    assert_eq!(literal.source_offset(6), 12);
    assert_eq!(literal.content.start, 1);
    assert_eq!(literal.content.end, 12);
}

#[test]
fn an_empty_string_maps_to_its_content_start() {
    assert_eq!(literal(r#"x """#).source_offset(0), 3);
    assert_eq!(literal("\"\"\"\n  \"\"\"").source_offset(0), 4);
}
