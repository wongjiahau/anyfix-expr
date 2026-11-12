//! The formatter removes only redundant `.`, `:` and `( )`, lays the rest out
//! canonically, keeps every comment, and never changes the tree.

mod common;

use anyx::{ErrorCode, TriviaKind, format, parse};
use common::tree;

/// Formats `source`, asserting the result has the same tree, keeps every
/// comment, and formats to itself.
fn fmt(source: &str) -> String {
    let formatted = format(source).unwrap_or_else(|errors| panic!("{source:?}: {errors:?}"));
    assert_eq!(tree(&formatted), tree(source), "tree of {source:?}");
    assert_eq!(
        comments(&formatted),
        comments(source),
        "comments of {source:?}"
    );
    assert_eq!(
        format(&formatted).unwrap(),
        formatted,
        "not idempotent: {source:?}"
    );
    formatted
}

fn comments(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    let parsed = parse(source);
    let mut collect = |token: &anyx::Token| {
        for trivia in token.leading.iter().chain(&token.trailing) {
            if trivia.kind == TriviaKind::Comment {
                found.push(trivia.text.clone());
            }
        }
    };
    visit_sequence(&parsed.syntax.sequence, &mut collect);
    collect(&parsed.syntax.end);
    found
}

fn visit_sequence(sequence: &anyx::Sequence, f: &mut impl FnMut(&anyx::Token)) {
    for element in &sequence.elements {
        visit_expression(&element.expression, f);
        if let Some(comma) = &element.comma {
            f(comma);
        }
    }
}

fn visit_expression(expression: &anyx::Expression, f: &mut impl FnMut(&anyx::Token)) {
    use anyx::{Expression, Item};
    match expression {
        Expression::Run(run) => {
            for item in &run.items {
                match item {
                    Item::Atom(token) | Item::Skipped(token) => f(token),
                    Item::Group(group) => {
                        f(&group.open);
                        visit_sequence(&group.sequence, f);
                        if let Some(close) = &group.close {
                            f(close);
                        }
                    }
                }
            }
        }
        Expression::Dot(binary) | Expression::Colon(binary) => {
            visit_expression(&binary.left, f);
            f(&binary.operator);
            visit_expression(&binary.right, f);
        }
    }
}

#[test]
fn redundant_sugar_and_parens_are_removed() {
    let cases = [
        ("1 . f", "1 f\n"),
        ("f : 1", "f 1\n"),
        ("f : [a]", "f [a]\n"),
        ("f ((1))", "f 1\n"),
        ("f ((x))", "f (x)\n"),
        ("(x)", "x\n"),
        ("(a . b) . c", "a . b . c\n"),
        ("1 .+ 2", "1 + 2\n"),
        ("a .+ b .+ c", "a + b .+ c\n"),
        ("f : 1 . g", "f: 1 g\n"),
        (
            "IF (x > 0) THEN (x) ELSE (0 - x)",
            "IF x > 0 THEN x ELSE 0 - x\n",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(fmt(source), expected, "{source:?}");
    }
}

#[test]
fn needed_sugar_and_parens_stay() {
    let cases = [
        "x . f\n",
        "f: x\n",
        "f (x)\n",
        "f (_)\n",
        "(x,)\n",
        "x . y _\n",
        "a . (b . c)\n",
        "x < (+)\n",
        "IF (T) THEN y\n",
        "LET s BE: IF a THEN b ELSE c\n",
        "a: b: c\n",
    ];
    for source in cases {
        assert_eq!(fmt(source), source, "{source:?}");
    }
}

#[test]
fn operators_are_spaced_canonically() {
    assert_eq!(fmt("a:b"), "a: b\n");
    assert_eq!(fmt("a   .   f"), "a . f\n");
    assert_eq!(fmt("x.+y .- z"), "x + y .- z\n");
    assert_eq!(fmt("f(x)"), "f (x)\n");
    assert_eq!(fmt("{a:b}"), "{ a: b }\n");
    assert_eq!(fmt("[ a , b , ]"), "[a, b]\n");
    assert_eq!(fmt("( a , b )"), "(a, b)\n");
    assert_eq!(fmt("{ }"), "{}\n");
}

#[test]
fn top_level_elements_get_their_own_lines() {
    assert_eq!(fmt("a, b"), "a,\nb,\n");
    assert_eq!(fmt("a,"), "a\n");
    assert_eq!(fmt(""), "");
}

#[test]
fn long_sequences_break_with_a_trailing_comma() {
    let long = (0..30)
        .map(|i| format!("item{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let formatted = fmt(&format!("list [{long}]"));
    assert!(
        formatted.starts_with("list [\n    item0,\n    item1,\n"),
        "{formatted}"
    );
    assert!(formatted.ends_with("    item29,\n]\n"), "{formatted}");
    assert!(
        formatted
            .lines()
            .all(|line| line.chars().count() <= anyx::WIDTH)
    );
}

#[test]
fn a_lone_element_in_parens_keeps_its_comma_or_its_absence() {
    let long = "x ".repeat(60);
    let grouped = fmt(&format!("f ({long})"));
    assert!(grouped.trim_end().ends_with("x\n)"), "{grouped}");
    let parens = fmt(&format!("f ({long},)"));
    assert!(parens.trim_end().ends_with("x,\n)"), "{parens}");
}

#[test]
fn long_dot_chains_break_before_each_dot() {
    let source = format!("{} . first step . second step", "word ".repeat(18));
    let formatted = fmt(&source);
    assert!(
        formatted.contains("\n    . first step\n    . second step\n"),
        "{formatted}"
    );
}

#[test]
fn blank_lines_collapse_to_one() {
    assert_eq!(fmt("a,\n\n\n\nb"), "a,\n\nb,\n");
    assert_eq!(fmt("{\n\n a,\n\n\n b }"), "{\n    a,\n\n    b,\n}\n");
}

#[test]
fn comments_are_kept_in_place() {
    assert_eq!(
        fmt("# head\n\na, # after a\nb"),
        "# head\n\na, # after a\nb,\n"
    );
    assert_eq!(
        fmt("{ a, # one\n  # own line\n  b,\n  # before close\n}"),
        "{\n    a, # one\n    # own line\n    b,\n    # before close\n}\n"
    );
    assert_eq!(fmt("a\n# the end\n"), "a\n# the end\n");
    assert_eq!(fmt("{ # open\n a }"), "{ # open\n    a,\n}\n");
}

#[test]
fn comments_on_removed_tokens_move_to_a_neighbour() {
    assert_eq!(fmt("f ( # note\n 1 )"), "f # note\n    1\n");
    assert_eq!(fmt("1 # one\n . f"), "1 # one\n    f\n");
    assert_eq!(
        fmt("[a # one\n # two\n , b]"),
        "[\n    a, # one\n    # two\n    b,\n]\n"
    );
}

#[test]
fn comments_never_swallow_code() {
    // A comment in the middle of a run ends its line, and the run continues
    // one level in. So does one before a raw string, which must open a line's end.
    assert_eq!(fmt("f # c\n x"), "f # c\n    x\n");
    assert_eq!(fmt("x . f # c\n . g"), "x . f # c\n    . g\n");
    let formatted = fmt("f # c\n \"\"\"\n  raw\n  \"\"\"");
    assert_eq!(
        formatted,
        "f # c\n    \"\"\"\n        raw\n        \"\"\"\n"
    );
}

#[test]
fn raw_strings_are_reindented_with_their_value_kept() {
    let source = "score {\n        voice \"\"\"\n          bpm=90 # content\n\n            [S1] 3 3\n          \"\"\" }";
    let formatted = fmt(source);
    assert_eq!(
        formatted,
        "score {\n    voice \"\"\"\n        bpm=90 # content\n\n          [S1] 3 3\n        \"\"\",\n}\n"
    );
    assert_eq!(
        fmt("x \"\"\"\r\n  a\r\n  b\r\n  \"\"\""),
        "x \"\"\"\n    a\r\n    b\n    \"\"\"\n"
    );
    assert_eq!(fmt("x \"\"\"\n\"\"\""), "x \"\"\"\n    \"\"\"\n");
    assert_eq!(
        fmt("x \"\"\"\"\n  \"\"\"\n  \"\"\"\""),
        "x \"\"\"\"\n    \"\"\"\n    \"\"\"\"\n"
    );
}

#[test]
fn documents_with_errors_are_not_formatted() {
    let errors = format("f (a").unwrap_err();
    assert_eq!(errors[0].code, ErrorCode::UnclosedBracket);
}

#[test]
fn fixtures_format_stably() {
    for name in ["config", "grammar", "json", "rg"] {
        let path = format!("{}/tests/fixtures/{name}.anyx", env!("CARGO_MANIFEST_DIR"));
        fmt(&std::fs::read_to_string(path).unwrap());
    }
}

/// Random documents built from every kind of token, comments and line breaks
/// included. Those that parse without errors must format to the same tree.
#[test]
fn random_documents_format_to_the_same_tree() {
    const PIECES: [&str; 32] = [
        "x",
        "y z",
        "IF",
        "THEN",
        "T",
        "+",
        "<=",
        "-",
        "_",
        "1",
        "2.5",
        "\"s\"",
        ".",
        ".",
        ":",
        ":",
        "(",
        ")",
        "(",
        ")",
        "[",
        "]",
        "{",
        "}",
        ",",
        " ",
        "\n",
        "\n\n",
        "# c\n",
        "( _ )",
        "\"\"\"\n  raw\n  \"\"\"",
        "(a,)",
    ];
    let mut state: u64 = 0x1234_5678_9ABC_DEF0;
    let mut next = |bound: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % bound as u64) as usize
    };
    let mut formatted = 0;
    for _ in 0..20_000 {
        let length = 1 + next(24);
        let source: String = (0..length)
            .map(|_| format!("{} ", PIECES[next(PIECES.len())]))
            .collect();
        if parse(&source).errors.is_empty() {
            fmt(&source);
            formatted += 1;
        }
    }
    assert!(
        formatted > 500,
        "only {formatted} random documents were valid"
    );
}
