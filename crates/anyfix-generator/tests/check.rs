//! What the generator reports about a grammar before generating from it.

mod common;

use anyfix_generator::check;
use common::{assert_snapshot, fixture};

/// Every error and warning for `source`, as rendered for a terminal.
fn report(source: &str) -> String {
    let diagnostics = match check(source) {
        Ok(checked) => checked.warnings,
        Err(errors) => errors,
    };
    if diagnostics.is_empty() {
        return "nothing to report\n".to_string();
    }
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.render(source, "grammar") + "\n")
        .collect()
}

fn reports(sources: &[&str]) -> String {
    sources
        .iter()
        .map(|source| format!("## {source}\n{}\n", report(source)))
        .collect()
}

#[test]
fn the_shipped_grammars_have_nothing_to_report() {
    for path in [
        "grammar.anyfix",
        "tests/fixtures/json.anyfix",
        "tests/fixtures/manifest.anyfix",
    ] {
        assert_eq!(report(&fixture(path)), "nothing to report\n", "{path}");
    }
}

#[test]
fn syntax_errors_come_from_anyfix() {
    assert_snapshot(
        "check_syntax.txt",
        &reports(&["ENTRY: (", "ENTRY: value,, value: STRING"]),
    );
}

#[test]
fn rules_must_exist_once_and_be_used() {
    assert_snapshot(
        "check_rules.txt",
        &reports(&[
            "ENTRY: valeu, value: STRING",
            "ENTRY: thing, value: STRING",
            "ENTRY: value, value: STRING, value: NUMBER",
            "ENTRY: value, value: STRING, other: NUMBER",
            "ENTRY: document, document: STRING",
        ]),
    );
}

#[test]
fn fields_and_choices_need_names() {
    assert_snapshot(
        "check_names.txt",
        &reports(&[
            "ENTRY: value, value: ONE OF [STRING, NUMBER]",
            "ENTRY: value, value: ONE OF [a: STRING, a: NUMBER]",
            "ENTRY: value, value: ONE OF [WORD \"-\", WORD \"null\", WORD \"IF\"]",
            "ENTRY: value, value: SQUARE BRACKET [STRING, value, value]",
            "ENTRY: value, value: CURLY BRACE [a: SQUARE BRACKET [STRING, NUMBER]]",
            "ENTRY: value, value: CURLY BRACE [a: SQUARE BRACKET [STRING]]",
            "ENTRY: value, value: ANYFIX CALL [WORD \"a\", span: STRING, self: STRING]",
        ]),
    );
}

#[test]
fn repetitions_belong_to_sequences() {
    assert_snapshot(
        "check_repetitions.txt",
        &reports(&[
            "ENTRY: value, value: ZERO OR MORE: STRING",
            "ENTRY: value, value: ONE OF [a: STRING, ZERO OR MORE: value]",
            "ENTRY: value, value: SQUARE BRACKET [ZERO OR MORE: OPTIONAL: value]",
            "ENTRY: value, value: ANYFIX CALL [OPTIONAL: WORD \"a\", x: STRING]",
            "ENTRY [ZERO OR MORE: value], value: SQUARE BRACKET [text: OPTIONAL: STRING]",
            "ENTRY: value, value: EACH: STRING",
            "ENTRY [EACH: value, last: value], value: STRING",
            "ENTRY: value, value: ANYFIX CALL [WORD \"f\", items: EACH: STRING, WORD \"end\"]",
            "ENTRY [EACH: value], value: SQUARE BRACKET [first: STRING, rest: EACH: NUMBER]",
        ]),
    );
}

#[test]
fn rules_must_not_recurse_without_matching() {
    assert_snapshot(
        "check_recursion.txt",
        &reports(&[
            "ENTRY: value, value: value",
            "ENTRY: a, a: b, b: ONE OF [x: STRING, a]",
            "ENTRY: value, value: ONE OF [x: STRING, list: SQUARE BRACKET [ZERO OR MORE: value]]",
        ]),
    );
}

#[test]
fn patterns_must_be_able_to_match() {
    assert_snapshot(
        "check_patterns.txt",
        &reports(&[
            "ENTRY: value, value: STRING [MATCHING REGEX \"(\"]",
            "ENTRY: value, value: NUMBER [AT LEAST 5, AT MOST 1]",
            "ENTRY: value, value: NUMBER [INTEGER, AT LEAST 0.2, AT MOST 0.8]",
            "ENTRY: value, value: NUMBER [INTEGER, INTEGER]",
            "ENTRY: value, value: NUMBER [SIGNED, SIGNED]",
            "ENTRY: value, value: WORD \"a b\"",
            "ENTRY: value, value: WORD \"\"",
            "ENTRY: value, value: ANY WORD [KEYWORD, SYMBOL]",
            "ENTRY: value, value: ANYFIX CALL [x: STRING]",
            "ENTRY: value, value: ANYFIX CALL [NAME [CONTAINS \"a\"], NAME [CONTAINS \"b\"]]",
            "ENTRY: value, value: ANYFIX CALL [NO ARGUMENTS, ARGUMENTS: STRING]",
            "ENTRY: value, value: ONE OF [x: STRING, group: PARENTHESIS: value]",
            "ENTRY: value, value: ONE OF [x: STRING, pair: PARENTHESIS [value, other: value]]",
        ]),
    );
}

#[test]
fn choices_that_compete_get_a_warning() {
    assert_snapshot(
        "check_choices.txt",
        &reports(&[
            "ENTRY: value, value: ONE OF [a: STRING, b: STRING \"x\"]",
            "ENTRY: value, value: ONE OF [a: STRING \"x\", b: STRING \"y\"]",
            "ENTRY: value, value: ONE OF [a: STRING [STARTS WITH \"x\"], b: STRING [ENDS WITH \"y\"]]",
            "ENTRY: value, value: ONE OF [a: STRING [STARTS WITH \"x\"], b: STRING [STARTS WITH \"y\"]]",
            "ENTRY: value, value: ONE OF [small: NUMBER [INTEGER, AT MOST 255], big: NUMBER]",
            "ENTRY: value, value: ONE OF [a: NUMBER [AT MOST 1], b: NUMBER [AT LEAST 2]]",
            "ENTRY: value, value: ONE OF [call: ANYFIX CALL, WORD \"null\"]",
            "ENTRY: value, value: ONE OF [WORD \"null\", call: ANYFIX CALL [ARGUMENTS: ONE OR MORE: value]]",
            "ENTRY: value, value: ONE OF [a: ANYFIX CALL [WORD \"f\", x: STRING], b: ANYFIX CALL [WORD \"f\", ZERO OR MORE: value]]",
            "ENTRY: value, value: ONE OF [a: ANYFIX CALL [WORD \"f\", x: STRING], b: ANYFIX CALL [WORD \"f\", ZERO OR MORE: text]], text: STRING",
            "ENTRY: value, value: ONE OF [a: SQUARE BRACKET [ZERO OR MORE: STRING], b: SQUARE BRACKET [ZERO OR MORE: NUMBER]]",
            "ENTRY: value, value: ONE OF [a: SQUARE BRACKET [ONE OR MORE: STRING], b: SQUARE BRACKET [ONE OR MORE: NUMBER]]",
            "ENTRY: value, value: ONE OF [a: SQUARE BRACKET [EACH: STRING], b: SQUARE BRACKET [ONE OR MORE: NUMBER]]",
            "ENTRY: value, value: ONE OF [a: ANYFIX CALL [NAME [STARTS WITH \"get \"]], b: ANYFIX CALL [WORD \"set\", value]]",
            "ENTRY: value, value: ONE OF [a: ANYFIX CALL [NAME [STARTS WITH \"get \"]], b: ANYFIX CALL [WORD \"get\", value]]",
            "ENTRY: value, value: ONE OF [a: ANY WORD [NAME], b: ANY WORD [KEYWORD], WORD \"IF\"]",
            "ENTRY: value, value: ONE OF [a: ANYFIX CALL [NAME [MATCHING REGEX \"^[a-z]+ _$\"]], b: ANYFIX CALL [NAME [MATCHING REGEX \"^[a-z]+$\"]]]",
            "ENTRY: value, value: ONE OF [a: ANYFIX CALL [NAME [MATCHING REGEX \"^[a-z]+\"]], b: ANYFIX CALL [NAME [MATCHING REGEX \"[a-z]+$\"]]]",
            "ENTRY: value, value: ONE OF [null: WORD \"null\", minus: ANYFIX CALL [WORD \"-\", n: NUMBER], words: ANYFIX CALL [NAME [MATCHING REGEX \"^[a-z]+$\"]], other: ANYFIX CALL]",
            "ENTRY: value, value: ONE OF [field: ANYFIX CALL [NAME [ENDS WITH \" _\"], ARGUMENTS: STRING], other: ANYFIX CALL [NAME [ENDS WITH \" _\"]]]",
            "ENTRY: value, value: ONE OF [a: ANYFIX CALL [WORD \"f\", x: ZERO OR MORE: STRING], other: ANYFIX CALL [NAME [STARTS WITH \"f\"]]]",
            "ENTRY: value, value: ONE OF [other: ANYFIX CALL, null: WORD \"null\"]",
            "ENTRY: value, value: ONE OF [words: ANYFIX CALL [NO ARGUMENTS], field: ANYFIX CALL [NAME [ENDS WITH \" _\"]]]",
            "ENTRY: value, value: ONE OF [words: ANYFIX CALL [NO ARGUMENTS], null: WORD \"null\"]",
            "ENTRY: value, value: ONE OF [null: WORD \"null\", words: ANYFIX CALL [NO ARGUMENTS]]",
            "ENTRY: value, value: ONE OF [any: ANY NODE, text: STRING]",
            "ENTRY: value, value: ONE OF [text: STRING, list: SQUARE BRACKET [ZERO OR MORE: value], any: ANY NODE]",
            "ENTRY: value, value: ONE OF [integer: integer, other: ANYFIX CALL], integer: ONE OF [number: NUMBER, minus: ANYFIX CALL [WORD \"-\", n: NUMBER]]",
            "ENTRY: value, value: ONE OF [signed: NUMBER [SIGNED], minus: ANYFIX CALL [WORD \"-\", n: NUMBER]]",
            "ENTRY: value, value: ONE OF [signed: NUMBER [SIGNED], other: ANYFIX CALL]",
            "ENTRY: value, value: ONE OF [signed: NUMBER [SIGNED], words: ANYFIX CALL [NO ARGUMENTS]]",
        ]),
    );
}

#[test]
fn any_order_takes_named_elements_that_compete_with_none() {
    assert_snapshot(
        "check_any_order.txt",
        &reports(&[
            "ENTRY: ANY ORDER [WORD \"x\", n: NUMBER]",
            "ENTRY: ANY ORDER [items: EACH: NUMBER]",
            "ENTRY: ANY ORDER [errors: OPTIONAL: NUMBER]",
            "ENTRY: ANY ORDER [name: OPTIONAL: STRING [MATCHING REGEX \"^a\"], text: ZERO OR MORE: STRING]",
            "ENTRY: thing, thing: CURLY BRACE: ANY ORDER [text: STRING, name: STRING [MATCHING REGEX \"^a\"]]",
            "ENTRY: ANY ORDER [count: NUMBER, label: OPTIONAL: STRING, tag: ZERO OR MORE: ANYFIX CALL [WORD \"tag\", value: STRING]]",
        ]),
    );
}
