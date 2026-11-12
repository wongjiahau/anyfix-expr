use anyfix_expr::{FlatWordKind, parse};

fn flat(source: &str) -> Vec<(String, FlatWordKind)> {
    let nodes = parse(source).syntax.nodes();
    nodes[0]
        .flat_words()
        .expect("only words")
        .into_iter()
        .map(|word| (word.text, word.kind))
        .collect()
}

#[test]
fn a_call_split_by_keywords_reads_back_in_source_order() {
    assert_eq!(
        flat("C3 to G4"),
        [
            ("C3".to_string(), FlatWordKind::Keyword),
            ("to".to_string(), FlatWordKind::Name),
            ("G4".to_string(), FlatWordKind::Keyword),
        ]
    );
}

#[test]
fn symbols_numbers_and_strings_keep_their_kind() {
    assert_eq!(
        flat(r#"A1 & "F#4" 20"#),
        [
            ("A1".to_string(), FlatWordKind::Keyword),
            ("&".to_string(), FlatWordKind::Symbol),
            ("F#4".to_string(), FlatWordKind::String),
            ("20".to_string(), FlatWordKind::Number),
        ]
    );
}

#[test]
fn a_string_word_spans_its_text_without_the_quotes() {
    let source = r#"x "abc""#;
    let nodes = parse(source).syntax.nodes();
    let words = nodes[0].flat_words().expect("only words");
    assert_eq!(&source[words[1].span.start..words[1].span.end], "abc");
}

#[test]
fn a_bracket_inside_is_not_a_word() {
    let nodes = parse("x [1]").syntax.nodes();
    let found = nodes[0].flat_words().expect_err("a bracket");
    assert_eq!(found.span.start, 2);
}
