#[path = "../examples/poentaro/generated.rs"]
#[rustfmt::skip]
#[allow(clippy::all, dead_code)]
mod generated;

anyx_lsp::language!(Poentaro = generated);

use anyx_lsp::Language;

fn labels(source_with_cursor: &str) -> Vec<String> {
    let offset = source_with_cursor.find('|').unwrap();
    let source = source_with_cursor.replace('|', "");
    anyx_lsp::complete(&Poentaro, &source, offset)
        .into_iter()
        .map(|completion| completion.snippet)
        .collect()
}

#[test]
fn it_completes_the_words_the_grammar_allows_at_the_cursor() {
    let sections = [
        "could not convert",
        "metadata",
        "parts",
        "sequence",
        "preset",
        "score",
    ];
    assert_eq!(labels("|"), sections);
    assert_eq!(labels("metadata { title \"x\" },\n|"), sections);
    let kinds = ["notes", "chords", "percussion", "follow", "lyrics"];
    assert_eq!(labels("parts [\"Melody\": |]"), kinds);
    assert_eq!(labels("parts [\"Melody\": no|]"), kinds);
    assert_eq!(
        labels("parts [\"Melody\": notes { |}]"),
        ["instrument", "range", "sound", "volume", "octave"]
    );
}

#[test]
fn it_completes_metadata_preset_and_style_keys() {
    assert_eq!(labels("metadata { max mea| }"), ["measures per system"]);
    let metadata = labels("metadata { | }");
    assert!(
        metadata.contains(&"max measures per system".to_string()),
        "{metadata:?}"
    );
    assert!(
        metadata.contains(&"lyrics style".to_string()),
        "{metadata:?}"
    );
    assert!(
        !metadata.contains(&"visible parts".to_string()),
        "{metadata:?}"
    );
    let preset = labels("preset \"Solo\" { | }");
    assert!(preset.contains(&"visible parts".to_string()), "{preset:?}");
    assert!(preset.contains(&"title".to_string()), "{preset:?}");
    let style = labels("metadata { lyrics style { | } }");
    assert_eq!(
        style,
        [
            "font size",
            "horizontal padding pt",
            "vertical padding pt",
            "bold",
            "italic",
            "underline",
            "font family"
        ]
    );
}

#[test]
fn it_reports_every_mistake_in_a_list() {
    let source = "parts [\"Melody\": nots, \"Bass\": chords, \"X\": foo],\nwhat";
    let found: Vec<&str> = Poentaro
        .check(source)
        .iter()
        .map(|problem| &source[problem.span.start..problem.span.end])
        .collect();
    assert_eq!(found, ["nots", "foo", "what"]);
}

#[test]
fn syntax_errors_come_from_anyx() {
    let problems = Poentaro.check("parts [\"Melody\": notes");
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].kind, anyx_lsp::ProblemKind::Syntax);
}

#[test]
fn it_folds_brackets_and_strings_over_lines() {
    let source = "metadata { title \"x\" },\nscore \"\"\"\n    1 2 3\n    \"\"\",\nparts [\n  \"M\": notes,\n]";
    assert_eq!(anyx_lsp::folds(source).len(), 2);
}

#[test]
fn it_completes_the_value_a_key_takes() {
    assert_eq!(labels("metadata { hide resting parts : | }"), ["yes", "no"]);
    assert_eq!(
        labels("metadata { lyrics style : { bold : | } }"),
        ["yes", "no"]
    );
    assert_eq!(
        labels("metadata { title style : { font family : | } }"),
        ["serif", "sans serif", "monospace"]
    );
    assert_eq!(
        labels("preset \"Solo\" { notation : | }"),
        [
            "numbered",
            "violin",
            "cello",
            "abc",
            "guitar",
            "harmonica",
            "jianpu"
        ]
    );
}

#[test]
fn a_wrong_value_is_a_problem_naming_the_values_there_are() {
    let messages = |source: &str| {
        Poentaro
            .check(source)
            .into_iter()
            .map(|problem| problem.message)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        messages("metadata { lyrics style : { bold : maybe } }"),
        ["found the word `maybe`; expected the word `yes` or `no`"]
    );
    assert_eq!(
        messages("metadata { max measures per system : 0 }"),
        ["found the number `0`; expected an integer of at least 1"]
    );
}
