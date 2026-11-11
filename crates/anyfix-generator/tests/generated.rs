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

use common::fixture;

/// Generates from `grammar` and compares with `generated`. Run the tests
/// with `UPDATE_SNAPSHOTS=1` to write the file instead.
fn assert_generated(grammar: &str, generated: &str) -> Vec<String> {
    let source = fixture(grammar);
    let output = anyfix_generator::generate(&source).unwrap_or_else(|errors| {
        let errors: Vec<String> = errors
            .iter()
            .map(|error| error.render(&source, grammar))
            .collect();
        panic!("{}", errors.join("\n"))
    });
    let code = anyfix_generator::format(&output.code).expect("rustfmt is installed");
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
    let warnings = assert_generated("grammar.anyfix", "src/grammar.rs");
    assert_eq!(warnings, Vec::<String>::new());
}

#[test]
fn generating_twice_gives_the_same_text() {
    let source = fixture("grammar.anyfix");
    let first = anyfix_generator::generate(&source).unwrap();
    let second = anyfix_generator::generate(&source).unwrap();
    assert!(first == second);
}

#[test]
fn json_is_generated() {
    let warnings = assert_generated("tests/fixtures/json.anyfix", "tests/generated/json.rs");
    assert_eq!(warnings, Vec::<String>::new());
}

#[test]
fn the_keyword_language_is_generated() {
    let warnings = assert_generated(
        "tests/fixtures/language.anyfix",
        "tests/generated/language.rs",
    );
    assert_eq!(warnings, Vec::<String>::new());
}

#[test]
fn the_config_format_is_generated() {
    let warnings = assert_generated("tests/fixtures/config.anyfix", "tests/generated/config.rs");
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
    assert_eq!(server.name.value, "server _");
    let settings: Vec<(&str, &str)> = server
        .arguments
        .value
        .iter()
        .map(|setting| match setting {
            Setting::Flag { name, .. } => (name.value.as_str(), "flag"),
            Setting::Pair {
                name, arguments, ..
            } => (
                name.value.as_str(),
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

/// The skill for agents carries its own copies of the example grammars.
#[test]
fn the_examples_of_the_skill_are_the_tested_grammars() {
    for name in ["json", "config", "language"] {
        assert_eq!(
            fixture(&format!(
                "../../skills/anyfix-grammar/examples/{name}.anyfix"
            )),
            fixture(&format!("tests/fixtures/{name}.anyfix")),
            "{name}"
        );
    }
}

#[test]
fn the_corners_are_generated() {
    assert_generated(
        "tests/fixtures/corners.anyfix",
        "tests/generated/corners.rs",
    );
}

fn parse(source: &str) -> Vec<anyfix_expr::Node> {
    let parsed = anyfix_expr::parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    parsed.syntax.nodes()
}

#[test]
fn json_decodes_into_its_tree() {
    let source = fixture("../anyfix-expr/tests/fixtures/json.anyfix");
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
    let written = |span: anyfix_expr::Span| &source[span.start..span.end];
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
    assert_eq!(name.value, "print _");
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
    assert_eq!(name.value, "move _ to _");
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
    let source = "[255, -1000, 0x10, 1_000, 7, 0.5, 42, 1e3, -2.5]";
    let document = corners::decode_document(&parse(source)).unwrap();
    let corners::Node::Numbers(numbers) = &document.node[0] else {
        panic!("{:?}", document.node);
    };
    let byte: u8 = numbers.byte.value;
    let offset: i16 = numbers.offset.value;
    let count: u64 = numbers.count.value;
    let whole: i64 = numbers.whole.value;
    let ratio: f64 = numbers.ratio.value;
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
    decode: impl Fn(&[anyfix_expr::Node]) -> Result<T, common_error::DecodeError>,
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
    pub struct DecodeError(pub String, pub anyfix_expr::Span);

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
            "WORDS [IF, name, +, x, ->, |, |, end]",
            "WORDS [if, name, +, x, ->, end]",
            "WORDS [IF, Name, +, x, ->, end]",
            "WORDS [IF, name, +, x, ->]",
            "(wrap, [type], finish)",
            "(only, word)",
            "raw 1 2, rawer 1",
            "{ type, NOT_NULL, span }",
        ],
        |nodes| Ok(corners::decode_document(nodes)?),
    );
    common::assert_snapshot("decode_corners.txt", &report);
}
