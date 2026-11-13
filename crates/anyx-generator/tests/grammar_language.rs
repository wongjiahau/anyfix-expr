//! Decoding grammars into the grammar language's typed tree.

use anyx::Span;
use anyx_generator::grammar::{
    Contents, Document, Element, Named, Number, NumberCondition, Pattern, Repetition,
    String as Text, decode_document,
};

fn decode(source: &str) -> Result<Document, String> {
    let parsed = anyx::parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    decode_document(&parsed.syntax.nodes()).map_err(|error| error.render(source, "grammar"))
}

fn fixture(path: &str) -> String {
    let path = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// The name as written: `value _` when plain, `value` when documented.
fn name_text(named: &Named) -> &str {
    match named {
        Named::Documented { name, .. } => name.value.as_str(),
        Named::Plain { name, .. } => name.text.as_str(),
    }
}

fn name_span(named: &Named) -> Span {
    match named {
        Named::Documented { name, .. } => name.span,
        Named::Plain { name, .. } => name.span,
    }
}

fn body(named: &Named) -> &Pattern {
    match named {
        Named::Documented { pattern, .. } => pattern,
        Named::Plain { arguments, .. } => arguments,
    }
}

fn rule_names(document: &Document) -> Vec<&str> {
    document.rule.iter().map(name_text).collect()
}

#[test]
fn the_grammar_of_grammars_decodes() {
    let document = decode(&fixture("grammar.anyx")).unwrap();
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
            "any order _",
            "element _",
            "pattern _",
            "reference _",
            "one of _",
            "repetition _",
            "container _",
            "specific call _",
            "format rule _",
            "break before _",
            "any call _",
            "call condition _",
            "name condition _",
            "word _",
            "exact words _",
            "words _",
            "any node _",
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
    let document = decode(&fixture("tests/fixtures/json.anyx")).unwrap();
    assert_eq!(
        rule_names(&document),
        ["value _", "object _", "member _", "array _"]
    );
    let Pattern::OneOf(value) = body(&document.rule[0]) else {
        panic!("value is a ONE OF");
    };
    let choices: Vec<&str> = value
        .choices
        .iter()
        .map(|choice| match choice {
            Element::Field(field) => name_text(field),
            Element::Pattern(Pattern::Reference(reference)) => reference.name.text.as_str(),
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
    let text = |span: anyx::Span| &source[span.start..span.end];
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
    assert_eq!(text(name_span(rule)), "size");
    let Pattern::Number(Number::Conditions { conditions, .. }) = body(rule) else {
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
    let Pattern::String(Text::Exact { text, .. }) = body(&document.rule[0]) else {
        panic!("value is an exact string");
    };
    assert_eq!(text.value.value, "a\n");
}

#[test]
fn a_line_string_above_a_rule_documents_it() {
    let source = "ENTRY: size,\n# How big it is,\n# in points.\nsize: NUMBER,\n## Only for whoever reads the grammar.\nname: STRING";
    let document = decode(source).unwrap();
    let Named::Documented {
        documentation,
        name,
        ..
    } = &document.rule[0]
    else {
        panic!("size is documented");
    };
    assert_eq!(name.value, "size");
    assert_eq!(documentation.value.value, "How big it is,\nin points.");
    assert!(
        matches!(&document.rule[1], Named::Plain { .. }),
        "a comment documents nothing"
    );
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
            "grammar:1:15: found the number `5`; expected the call `_ … _`, a call whose name matches the regex `^[a-z][a-z0-9]*( [a-z][a-z0-9]*)* _$` or the end of the document
  ENTRY: value, 5
                ^",
        ),
        (
            "ENTRY: value, value: ANYFIX CALL [WORD \"f\", NAME [STARTS WITH \"g\"]]",
            "grammar:1:45: found the word `NAME`; expected the word `ONE`, `ZERO`, `OPTIONAL`, `EACH`, `CURLY`, `SQUARE`, `PARENTHESIS`, `ANYFIX`, `WORD`, `ANY`, `WORDS`, `STRING` or `NUMBER`
  ENTRY: value, value: ANYFIX CALL [WORD \"f\", NAME [STARTS WITH \"g\"]]
                                              ^^^^",
        ),
        (
            "ENTRY: value, value: a_b",
            "grammar:1:22: found the word `a_b`; expected the word `ONE`, `ZERO`, `OPTIONAL`, `EACH`, `CURLY`, `SQUARE`, `PARENTHESIS`, `ANYFIX`, `WORD`, `ANY`, `WORDS`, `STRING`, `NUMBER` or a call whose name matches the regex `^[a-z][a-z0-9]*( [a-z][a-z0-9]*)*$`
  ENTRY: value, value: a_b
                       ^^^",
        ),
        (
            "ENTRY: value, value: (a, b)",
            "grammar:1:22: found `(...)`; expected the word `STRING`, `NUMBER`, the call `ONE OF _`, `ZERO OR MORE _`, `ONE OR MORE _`, `OPTIONAL _`, `EACH _`, `CURLY BRACE _`, `SQUARE BRACKET _`, `PARENTHESIS _`, `ANYFIX CALL _ FORMAT _`, `ANYFIX CALL _`, `ANYFIX CALL _?`, `WORD _`, `ANY WORD _?`, `WORDS _`, `WORDS _?`, `STRING _`, `NUMBER _`, `ANY NODE` or a call whose name matches the regex `^[a-z][a-z0-9]*( [a-z][a-z0-9]*)*$`
  ENTRY: value, value: (a, b)
                       ^^^^^^",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(decode(source).unwrap_err(), expected, "for {source:?}");
    }
}
