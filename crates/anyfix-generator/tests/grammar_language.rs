//! Decoding grammars into the grammar language's typed tree.

use anyfix_generator::grammar::{
    Contents, Document, Element, Number, NumberCondition, Pattern, Repetition, String as Text,
    decode_document,
};

fn decode(source: &str) -> Result<Document, String> {
    let parsed = anyfix_expr::parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    decode_document(&parsed.syntax.nodes()).map_err(|error| error.render(source, "grammar"))
}

fn fixture(path: &str) -> String {
    let path = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn rule_names(document: &Document) -> Vec<&str> {
    document
        .rule
        .iter()
        .map(|rule| rule.name.value.as_str())
        .collect()
}

#[test]
fn the_grammar_of_grammars_decodes() {
    let document = decode(&fixture("grammar.anyfix")).unwrap();
    let Contents::List(entry) = &document.entry.contents else {
        panic!("the entry is a list");
    };
    assert_eq!(entry.len(), 2);
    assert_eq!(
        rule_names(&document),
        [
            "entry _",
            "rule _",
            "named _",
            "contents _",
            "element _",
            "pattern _",
            "reference _",
            "one of _",
            "repetition _",
            "container _",
            "specific call _",
            "any call _",
            "call condition _",
            "word _",
            "any word _",
            "word condition _",
            "string _",
            "string condition _",
            "number _",
            "number condition _",
        ]
    );
}

#[test]
fn the_json_grammar_decodes() {
    let document = decode(&fixture("tests/fixtures/json.anyfix")).unwrap();
    assert_eq!(
        rule_names(&document),
        ["value _", "object _", "member _", "array _"]
    );
    let Pattern::OneOf(value) = &document.rule[0].arguments else {
        panic!("value is a ONE OF");
    };
    let choices: Vec<&str> = value
        .choices
        .iter()
        .map(|choice| match choice {
            Element::Field(field) => field.name.value.as_str(),
            Element::Pattern(Pattern::Reference(reference)) => reference.name.value.as_str(),
            Element::Pattern(Pattern::Word(word)) => word.text.value.value.as_str(),
            other => panic!("unexpected choice {other:?}"),
        })
        .collect();
    assert_eq!(
        choices,
        [
            "object", "array", "string _", "number _", "null", "true", "false"
        ]
    );
}

#[test]
fn every_node_has_its_span() {
    let source = "ENTRY: OPTIONAL: size, size: NUMBER [INTEGER, AT LEAST 0x10]";
    let document = decode(source).unwrap();
    let text = |span: anyfix_expr::Span| &source[span.start..span.end];
    assert_eq!(text(document.span), source);
    assert_eq!(text(document.entry.span), "ENTRY: OPTIONAL: size");
    let Contents::Element(Element::Pattern(Pattern::Repetition(optional))) =
        &document.entry.contents
    else {
        panic!("the entry is an optional");
    };
    assert_eq!(text(optional.span()), "OPTIONAL: size");
    let Repetition::Optional { pattern, .. } = optional else {
        panic!("the entry is an optional");
    };
    assert_eq!(text(pattern.span()), "size");
    let rule = &document.rule[0];
    assert_eq!(text(rule.name.span), "size");
    let Pattern::Number(Number::Conditions { conditions, .. }) = &rule.arguments else {
        panic!("size is a number with conditions");
    };
    assert_eq!(text(conditions.span), "[INTEGER, AT LEAST 0x10]");
    let NumberCondition::AtLeast { bound, .. } = &conditions[1] else {
        panic!("the second condition is AT LEAST");
    };
    assert_eq!(bound.value, 16.0);
    assert_eq!(text(bound.span), "0x10");
}

#[test]
fn strings_keep_their_decoded_value() {
    let document = decode("ENTRY: value, value: STRING \"a\\n\"").unwrap();
    let Pattern::String(Text::Exact { text, .. }) = &document.rule[0].arguments else {
        panic!("value is an exact string");
    };
    assert_eq!(text.value.value, "a\n");
}

/// Each bad grammar with the error it gives, as rendered for a terminal.
#[test]
fn bad_grammars_say_what_was_found_and_what_was_expected() {
    let cases = [
        (
            "ENTRY: value, value: ONE OFF [a, b]",
            "grammar:1:26: found the word `OFF`; expected the word `OF` or `OR`
  ENTRY: value, value: ONE OFF [a, b]
                           ^^^",
        ),
        (
            "ENTRY: value,\nvalue: STRING *",
            "grammar:2:15: found the word `*`; expected a string or `[...]`
  value: STRING *
                ^",
        ),
        (
            "value: STRING",
            "grammar:1:1: found the word `value`; expected the word `ENTRY`
  value: STRING
  ^^^^^",
        ),
        (
            "",
            "grammar:1:1: found the end of the document; expected an entry
  
  ^",
        ),
        (
            "ENTRY: value, value: STRING [STARTS \"a\"]",
            "grammar:1:37: found the string \"a\"; expected the word `WITH`
  ENTRY: value, value: STRING [STARTS \"a\"]
                                      ^^^",
        ),
        (
            "ENTRY: value, value: NUMBER [INTEGER, AT LEAST \"a\"]",
            "grammar:1:48: found the string \"a\"; expected a number
  ENTRY: value, value: NUMBER [INTEGER, AT LEAST \"a\"]
                                                 ^^^",
        ),
        (
            "ENTRY: value, value: ONE OF []",
            "grammar:1:30: found the closing `]`; expected an element
  ENTRY: value, value: ONE OF []
                               ^",
        ),
        (
            "ENTRY: value, value: ANY WORD [KEYWORDS]",
            "grammar:1:32: found the word `KEYWORDS`; expected the word `KEYWORD`, `NAME`, `SYMBOL`, `STARTS`, `ENDS`, `CONTAINS` or `MATCHING`
  ENTRY: value, value: ANY WORD [KEYWORDS]
                                 ^^^^^^^^",
        ),
        (
            "ENTRY: value, value: WORD null",
            "grammar:1:27: found the word `null`; expected a string
  ENTRY: value, value: WORD null
                            ^^^^",
        ),
        (
            "ENTRY: value, value: ANYFIX CALL [NAME \"x\"]",
            "grammar:1:40: found the string \"x\"; expected `[...]`
  ENTRY: value, value: ANYFIX CALL [NAME \"x\"]
                                         ^^^",
        ),
        (
            "ENTRY: value, 5",
            "grammar:1:15: found the number `5`; expected a call whose name matches the regex `^[a-z][a-z0-9]*( [a-z][a-z0-9]*)* _$` or the end of the document
  ENTRY: value, 5
                ^",
        ),
        (
            "ENTRY: value, value: a_b",
            "grammar:1:22: found the word `a_b`; expected the word `ONE`, `ZERO`, `OPTIONAL`, `EACH`, `CURLY`, `SQUARE`, `PARENTHESIS`, `SPECIFIC`, `ANYFIX`, `WORD`, `ANY`, `STRING`, `NUMBER` or a call whose name matches the regex `^[a-z][a-z0-9]*( [a-z][a-z0-9]*)*$`
  ENTRY: value, value: a_b
                       ^^^",
        ),
        (
            "ENTRY: value, value: (a, b)",
            "grammar:1:22: found `(...)`; expected the word `STRING`, `NUMBER`, the call `ONE OF _`, `ZERO OR MORE _`, `ONE OR MORE _`, `OPTIONAL _`, `EACH _`, `CURLY BRACE _`, `SQUARE BRACKET _`, `PARENTHESIS _`, `SPECIFIC ANYFIX CALL _`, `ANYFIX CALL _?`, `WORD _`, `ANY WORD _?`, `STRING _`, `NUMBER _` or a call whose name matches the regex `^[a-z][a-z0-9]*( [a-z][a-z0-9]*)*$`
  ENTRY: value, value: (a, b)
                       ^^^^^^",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(decode(source).unwrap_err(), expected, "for {source:?}");
    }
}
