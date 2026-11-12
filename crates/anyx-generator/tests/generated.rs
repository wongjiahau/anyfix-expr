//! The generator's output for the grammars in `tests/fixtures`, kept in
//! `tests/generated` and compiled here as modules, so that it is known to
//! build, to pass clippy and to decode.

mod common;

#[path = "generated/config.rs"]
mod config;
#[path = "generated/corners.rs"]
mod corners;
#[path = "generated/json.rs"]
mod json;
#[path = "generated/language.rs"]
mod language;
#[path = "generated/manifest.rs"]
mod manifest;

use common::fixture;

/// Generates from `grammar` and compares with `generated`. Run the tests
/// with `UPDATE_SNAPSHOTS=1` to write the file instead.
fn assert_generated(grammar: &str, generated: &str) -> Vec<String> {
    let source = fixture(grammar);
    let output = anyx_generator::generate(&source).unwrap_or_else(|errors| {
        let errors: Vec<String> = errors
            .iter()
            .map(|error| error.render(&source, grammar))
            .collect();
        panic!("{}", errors.join("\n"))
    });
    let code = anyx_generator::format(&output.code).expect("rustfmt is installed");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        let path = format!("{}/{generated}", env!("CARGO_MANIFEST_DIR"));
        std::fs::write(&path, &code).unwrap_or_else(|error| panic!("{path}: {error}"));
    } else {
        assert!(
            fixture(generated) == code,
            "{generated} is not what {grammar} generates; run with UPDATE_SNAPSHOTS=1 and review the diff"
        );
    }
    output
        .warnings
        .iter()
        .map(|warning| warning.render(&source, grammar))
        .collect()
}

#[test]
fn the_generator_generates_its_own_decoder() {
    let warnings = assert_generated("grammar.anyx", "src/grammar.rs");
    assert_eq!(warnings, Vec::<String>::new());
}

#[test]
fn generating_twice_gives_the_same_text() {
    let source = fixture("grammar.anyx");
    let first = anyx_generator::generate(&source).unwrap();
    let second = anyx_generator::generate(&source).unwrap();
    assert!(first == second);
}

#[test]
fn json_is_generated() {
    let warnings = assert_generated("tests/fixtures/json.anyx", "tests/generated/json.rs");
    assert_eq!(warnings, Vec::<String>::new());
}

#[test]
fn the_keyword_language_is_generated() {
    let warnings = assert_generated(
        "tests/fixtures/language.anyx",
        "tests/generated/language.rs",
    );
    assert_eq!(warnings, Vec::<String>::new());
}

#[test]
fn the_config_format_is_generated() {
    let warnings = assert_generated("tests/fixtures/config.anyx", "tests/generated/config.rs");
    assert_eq!(warnings, Vec::<String>::new());
}

#[test]
fn the_config_format_decodes() {
    use config::{Setting, Value};

    let source = "server {
  host: \"localhost\",
  port: 8080,
  allowed origins: [\"a\", \"b\"],
  verbose logging,
  tls { certificate: \"cert.pem\" },
}";
    let document = config::decode_document(&parse(source)).unwrap();
    let [server] = &document.section[..] else {
        panic!("{:?}", document.section);
    };
    assert_eq!(server.name.text, "server _");
    let settings: Vec<(&str, &str)> = server
        .arguments
        .value
        .iter()
        .map(|setting| match setting.as_ref().unwrap() {
            Setting::Flag { name, .. } => (name.text.as_str(), "flag"),
            Setting::Pair {
                name, arguments, ..
            } => (
                name.text.as_str(),
                match arguments {
                    Value::Text(_) => "text",
                    Value::Number(_) => "number",
                    Value::List(_) => "list",
                    Value::Section(_) => "section",
                },
            ),
        })
        .collect();
    assert_eq!(
        settings,
        [
            ("host _", "text"),
            ("port _", "number"),
            ("allowed origins _", "list"),
            ("verbose logging", "flag"),
            ("tls _", "section")
        ]
    );
}

#[test]
fn each_keeps_a_setting_that_does_not_fit_as_its_error() {
    let source = "server { port: 8080, \"oops\", tls { certificate: (1, 2) }, verbose logging }";
    let document = config::decode_document(&parse(source)).unwrap();
    let [server] = &document.section[..] else {
        panic!("{:?}", document.section);
    };
    let settings: Vec<Result<&str, String>> = server
        .arguments
        .value
        .iter()
        .map(|setting| match setting {
            Ok(config::Setting::Pair { name, .. } | config::Setting::Flag { name, .. }) => {
                Ok(name.text.as_str())
            }
            Err(error) => Err(error.render(source, "input")),
        })
        .collect();
    assert_eq!(
        settings,
        [
            Ok("port _"),
            Err("input:1:22: found the string \"oops\"; expected a call whose name matches the regex `^[a-z][a-z0-9 ]* _$` or a call whose name matches the regex `^[a-z][a-z0-9 ]*$`\n  server { port: 8080, \"oops\", tls { certificate: (1, 2) }, verbose logging }\n                       ^^^^^^".to_string()),
            Ok("tls _"),
            Ok("verbose logging"),
        ]
    );
}

/// The skill for agents carries its own copies of the example grammars.
#[test]
fn the_examples_of_the_skill_are_the_tested_grammars() {
    for name in ["json", "config", "language"] {
        assert_eq!(
            fixture(&format!("../../skills/anyx-grammar/examples/{name}.anyx")),
            fixture(&format!("tests/fixtures/{name}.anyx")),
            "{name}"
        );
    }
}

#[test]
fn the_corners_are_generated() {
    assert_generated("tests/fixtures/corners.anyx", "tests/generated/corners.rs");
}

fn parse(source: &str) -> Vec<anyx::Node> {
    let parsed = anyx::parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    parsed.syntax.nodes()
}

#[test]
fn json_decodes_into_its_tree() {
    let source = fixture("../anyx/tests/fixtures/json.anyx");
    let document = json::decode_document(&parse(&source)).unwrap();
    let json::Value::Object(object) = &document.value else {
        panic!("{:?}", document.value);
    };
    let keys: Vec<&str> = object
        .member
        .iter()
        .map(|member| member.key.value.value.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "name",
            "version",
            "private",
            "license",
            "description",
            "keywords",
            "repository",
            "release",
            "escapes",
            "contributors",
            "settings"
        ]
    );
    let value = |key: &str| {
        let member = object
            .member
            .iter()
            .find(|member| member.key.value.value == key);
        &member.unwrap().value
    };
    assert!(matches!(value("private"), json::Value::True(_)));
    assert!(matches!(value("license"), json::Value::Null(_)));
    let json::Value::Array(keywords) = value("keywords") else {
        panic!("{:?}", value("keywords"));
    };
    let keywords: Vec<&str> = keywords
        .value
        .iter()
        .map(|keyword| match keyword {
            json::Value::String(text) => text.value.value.as_str(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(keywords, ["shell", "cli", "language"]);
    let json::Value::Object(release) = value("release") else {
        panic!("{:?}", value("release"));
    };
    let json::Value::Object(latency) = &release.member[1].value else {
        panic!("{:?}", release.member[1]);
    };
    let numbers: Vec<f64> = latency
        .member
        .iter()
        .map(|member| match &member.value {
            json::Value::Number(number) => number.value,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(numbers, [1.25, 18.5, -3.0, 1e-5, 2.5e3]);

    // Every part knows where it was written.
    let written = |span: anyx::Span| &source[span.start..span.end];
    assert_eq!(written(object.member[0].key.span), "\"name\"");
    assert_eq!(written(object.member[0].span), "\"name\": \"resh\"");
    assert_eq!(
        written(value("keywords").span()),
        "[\"shell\", \"cli\", \"language\"]"
    );
}

#[test]
fn the_keyword_language_decodes() {
    use language::{Expression, Statement};

    let source = "LET total = 1,
IF total < 3 THEN print \"small\" ELSE { RETURN total },
move 1 to (total + 1),
RETURN";
    let document = language::decode_document(&parse(source)).unwrap();
    let [first, second, third, fourth] = &document.statement[..] else {
        panic!("{:?}", document.statement);
    };

    let Statement::Let { binding, .. } = first else {
        panic!("{first:?}");
    };
    assert_eq!(binding.name.value, "total");
    assert!(matches!(&binding.value, Expression::Number(number) if number.value == 1.0));

    let Statement::Expression(Expression::If {
        condition,
        then,
        r#else,
        ..
    }) = second
    else {
        panic!("{second:?}");
    };
    assert!(matches!(&**condition, Expression::Less { .. }));
    let Expression::Call {
        name, arguments, ..
    } = &**then
    else {
        panic!("{then:?}");
    };
    assert_eq!(name.text, "print _");
    assert!(matches!(&arguments[..], [Expression::Text(text)] if text.value.value == "small"));
    let Expression::Block(block) = &**r#else else {
        panic!("{:?}", r#else);
    };
    assert!(matches!(
        &block.value[..],
        [Statement::Return { expression: Some(Expression::Variable(name)), .. }]
            if name.value == "total"
    ));

    // A call of the user's own keeps its whole name, slots included.
    let Statement::Expression(Expression::Call {
        name, arguments, ..
    }) = third
    else {
        panic!("{third:?}");
    };
    assert_eq!(name.text, "move _ to _");
    assert!(matches!(
        &arguments[..],
        [Expression::Number(_), Expression::Sum { .. }]
    ));

    assert!(matches!(
        fourth,
        Statement::Return {
            expression: None,
            ..
        }
    ));
}

#[test]
fn numbers_decode_to_the_type_their_conditions_allow() {
    let source = "[255, -1000, 0x10, 1_000, 7, 0.5, 42, - 3, 1e3, -2.5]";
    let document = corners::decode_document(&parse(source)).unwrap();
    let corners::Node::Numbers(numbers) = &document.node[0] else {
        panic!("{:?}", document.node);
    };
    let byte: u8 = numbers.byte.value;
    let offset: i16 = numbers.offset.value;
    let count: u64 = numbers.count.value;
    let whole: i64 = numbers.whole.value;
    let ratio: f64 = numbers.ratio.value;
    let signed: i8 = numbers.signed.value;
    assert_eq!(signed, -3);
    assert_eq!(
        (byte, offset, count, whole, ratio),
        (255, -1000, 16, 1000, 0.5)
    );
    let rest: Vec<f64> = numbers.rest.iter().map(|number| number.value).collect();
    assert_eq!(rest, [1000.0, -2.5]);
}

/// The error each source gets from `decode`, as rendered for a terminal.
fn errors<T>(
    sources: &[&str],
    decode: impl Fn(&[anyx::Node]) -> Result<T, common_error::DecodeError>,
) -> String {
    sources
        .iter()
        .map(|source| {
            let report = match decode(&parse(source)) {
                Ok(_) => "decodes".to_string(),
                Err(error) => error.render(source, "input"),
            };
            format!("## {source}\n{report}\n\n")
        })
        .collect()
}

/// Each generated module has its own `DecodeError`; they are the same type
/// written three times, so the tests turn them into one.
mod common_error {
    pub struct DecodeError(pub String, pub anyx::Span);

    impl DecodeError {
        pub fn render(&self, source: &str, file: &str) -> String {
            let error = crate::json::DecodeError {
                span: self.1,
                found: String::new(),
                expected: Vec::new(),
            };
            let rendered = error.render(source, file);
            rendered.replacen("found ; expected nothing", &self.0, 1)
        }
    }

    macro_rules! from {
        ($module:ident) => {
            impl From<crate::$module::DecodeError> for DecodeError {
                fn from(error: crate::$module::DecodeError) -> Self {
                    DecodeError(error.message(), error.span)
                }
            }
        };
    }
    from!(json);
    from!(language);
    from!(corners);
}

#[test]
fn json_errors_point_at_the_source() {
    let report = errors(
        &[
            "",
            "1 2",
            "nul",
            "[1, yes]",
            "{\"a\": 1, b: 2}",
            "{\"a\" 1, 2}",
            "{\"a\": [true, {\"b\": (1)}]}",
            "[1, f(2)]",
        ],
        |nodes| Ok(json::decode_document(nodes)?),
    );
    common::assert_snapshot("decode_json.txt", &report);
}

#[test]
fn keyword_language_errors_point_at_the_source() {
    let report = errors(
        &[
            "LET 1 = 2",
            "LET x",
            "IF a THEN b",
            "IF a THEN b ELSE [c]",
            "RETURN 1 2",
            "WHILE a",
            "print (1, 2)",
            "{ LET x = 1, LET y }",
            "a + b + c",
        ],
        |nodes| Ok(language::decode_document(nodes)?),
    );
    common::assert_snapshot("decode_language.txt", &report);
}

#[test]
fn conditions_are_checked_where_the_value_is() {
    let report = errors(
        &[
            "[256, 0, 0, 0, 0, 0.5, 42]",
            "[1, -1001, 0, 0, 0, 0.5, 42]",
            "[1, 0, -1, 0, 0, 0.5, 42]",
            "[1, 0, 0, 1.5, 0, 0.5, 42]",
            "[1, 0, 0, 0, 0, 2, 42]",
            "[1, 0, 0, 0, 0, 0.5, 43]",
            "[1, 0, 0, 0, 0, 0.5, 42, \"x\"]",
            "\"#a\", [x]",
            "\"#a!\", \"exact\", \"other\"",
            "WORDS [IF, name, +, x, sans serif, ->, |, |, end]",
            "WORDS [if, name, +, x, y, ->, end]",
            "WORDS [IF, Name, +, x, y, ->, end]",
            "WORDS [IF, name, +, x, y, ->]",
            "WORDS [IF, name, +, x, print 1, ->, end]",
            "KEEP [1, x], KEEP 3, KEEP",
            "(wrap, [type], finish)",
            "(only, word)",
            "raw 1 2, rawer 1",
            "{ type, NOT_NULL, span }",
            "SET: MAX 1",
            "SET: 1 2",
            "SET: x \"y\"",
            "UNSET: OLD",
            "UNSET: old (1)",
        ],
        |nodes| Ok(corners::decode_document(nodes)?),
    );
    common::assert_snapshot("decode_corners.txt", &report);
}

#[test]
fn words_decode_to_their_text_after_an_optional_leading_part() {
    let source =
        "SET:\n  # The highest port.\n  max port 8080,\nSET: min port 1,\nUNSET: old thing";
    let document = corners::decode_document(&parse(source)).unwrap();
    let [
        corners::Node::Set { field: first, .. },
        corners::Node::Set { field: second, .. },
        corners::Node::Unset { name, .. },
    ] = &document.node[..]
    else {
        panic!("{:?}", document.node);
    };
    assert_eq!(
        first.doc.as_ref().map(|doc| doc.value.value.as_str()),
        Some("The highest port.")
    );
    assert_eq!(first.name.value, "max port");
    assert_eq!(
        &source[first.name.span.start..first.name.span.end],
        "max port"
    );
    assert_eq!(first.value.value, 8080.0);
    assert_eq!(second.doc, None);
    assert_eq!(second.name.value, "min port");
    assert_eq!(name.name.text, "old thing");
}

#[test]
fn a_call_name_gives_its_words_and_the_call_written_out_flat() {
    let source = "raw thing (5) \"x\"";
    let document = corners::decode_document(&parse(source)).unwrap();
    let corners::Node::Raw {
        name, arguments, ..
    } = &document.node[0]
    else {
        panic!("{:?}", document.node);
    };
    assert_eq!(name.text, "raw thing _ _");
    assert_eq!(name.words().collect::<Vec<_>>(), ["raw", "thing"]);
    let flat: Vec<String> = name
        .flat_words(arguments)
        .unwrap()
        .into_iter()
        .map(|word| word.text)
        .collect();
    assert_eq!(flat, ["raw", "thing", "5", "x"]);
    assert_eq!(&source[name.span.start..name.span.end], "raw thing");
}

#[test]
fn the_manifest_is_generated() {
    let warnings = assert_generated(
        "tests/fixtures/manifest.anyx",
        "tests/generated/manifest.rs",
    );
    assert_eq!(warnings, Vec::<String>::new());
}

#[test]
fn any_order_takes_the_elements_wherever_they_stand() {
    let source = "dependency \"a\",
license \"MIT\",
package { author \"x\", name \"demo\", author \"y\" },
dependency \"b\"";
    let document = manifest::decode_document(&parse(source)).unwrap();
    assert_eq!(document.errors, Vec::new());
    assert_eq!(document.package.fields.name.value.value.value, "demo");
    assert_eq!(document.package.fields.version, None);
    let authors: Vec<&str> = document
        .package
        .fields
        .author
        .iter()
        .map(|author| author.value.value.value.as_str())
        .collect();
    assert_eq!(authors, ["x", "y"]);
    let dependencies: Vec<&str> = document
        .dependency
        .iter()
        .map(|dependency| dependency.name.value.value.as_str())
        .collect();
    assert_eq!(dependencies, ["a", "b"]);
    assert_eq!(document.license.unwrap().name.value.value, "MIT");
}

#[test]
fn any_order_keeps_a_second_single_element_and_a_stray_node_as_errors() {
    let source = "license \"MIT\",
package { name \"demo\" },
license \"GPL\",
\"stray\"";
    let document = manifest::decode_document(&parse(source)).unwrap();
    assert_eq!(document.license.unwrap().name.value.value, "MIT");
    let messages: Vec<String> = document
        .errors
        .iter()
        .map(|error| error.message())
        .collect();
    assert_eq!(
        messages,
        [
            "found a second `license`; expected one `license` at most",
            "found the string \"stray\"; expected the call `package _`, `dependency _` or `license _`",
        ]
    );
    assert_eq!(
        &source[document.errors[0].span.start..document.errors[0].span.end],
        "license \"GPL\""
    );
}

#[test]
fn any_order_fails_without_a_required_element() {
    let error = manifest::decode_document(&parse("license \"MIT\"")).unwrap_err();
    assert_eq!(
        error.message(),
        "found the end of the document; expected the call `package _`"
    );
}

#[test]
fn parse_document_gives_the_syntax_errors_and_the_document() {
    let parsed = manifest::parse_document("package { name \"demo\" }, license (\"MIT\"");
    let messages: Vec<&str> = parsed
        .syntax_errors
        .iter()
        .map(|error| error.message.as_str())
        .collect();
    assert_eq!(messages.len(), 1, "{messages:?}");
    let document = parsed.document.unwrap();
    assert_eq!(document.package.fields.name.value.value.value, "demo");
}
