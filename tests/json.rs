//! Every JSON document parses with no errors, and its tree reads back as the
//! same JSON value. The documents are generated, covering every escape, number
//! form and whitespace character JSON allows.

mod common;

use anyfix_expr::{Node, NodeKind};
use common::nodes;

#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    /// The source text, sign included.
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(Vec<Member>),
}

#[derive(Clone, Debug, PartialEq)]
struct Member {
    key: String,
    value: Json,
}

/// A small deterministic random number generator (xorshift).
struct Random(u64);

impl Random {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const CHARACTERS: [char; 16] = [
    'a', 'Z', ' ', '"', '\\', '/', '\u{8}', '\u{c}', '\n', '\r', '\t', '\u{1}', '#', 'é', '中',
    '😀',
];

fn value(random: &mut Random, depth: usize) -> Json {
    let kinds = if depth == 0 { 4 } else { 6 };
    match random.below(kinds) {
        0 => random
            .pick(&[Json::Null, Json::Bool(true), Json::Bool(false)])
            .clone(),
        1 | 2 => Json::Number(number(random)),
        3 => Json::String(text(random)),
        4 => Json::Array(
            (0..random.below(4))
                .map(|_| value(random, depth - 1))
                .collect(),
        ),
        _ => Json::Object(
            (0..random.below(4))
                .map(|_| Member {
                    key: text(random),
                    value: value(random, depth - 1),
                })
                .collect(),
        ),
    }
}

/// Between `least` and `least + extra - 1` random digits.
fn digits(random: &mut Random, least: usize, extra: usize) -> String {
    let count = least + random.below(extra);
    (0..count)
        .map(|_| char::from(b'0' + random.below(10) as u8))
        .collect()
}

/// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`
fn number(random: &mut Random) -> String {
    let sign = *random.pick(&["", "-"]);
    let integer = match random.below(3) {
        0 => "0".to_string(),
        _ => format!("{}{}", 1 + random.below(9), digits(random, 0, 4)),
    };
    let fraction = match random.below(2) {
        0 => String::new(),
        _ => format!(".{}", digits(random, 1, 3)),
    };
    let exponent = match random.below(3) {
        0 => String::new(),
        _ => format!(
            "{}{}{}",
            random.pick(&["e", "E"]),
            random.pick(&["", "+", "-"]),
            digits(random, 1, 3)
        ),
    };
    format!("{sign}{integer}{fraction}{exponent}")
}

fn text(random: &mut Random) -> String {
    (0..random.below(6))
        .map(|_| *random.pick(&CHARACTERS))
        .collect()
}

fn space(random: &mut Random) -> &'static str {
    *random.pick(&["", " ", "  ", "\n", "\r\n", "\t", "\n  "])
}

fn write_string(random: &mut Random, text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        let short = match c {
            '"' => Some("\\\""),
            '\\' => Some("\\\\"),
            '/' => Some(*random.pick(&["/", "\\/"])),
            '\u{8}' => Some("\\b"),
            '\u{c}' => Some("\\f"),
            '\n' => Some("\\n"),
            '\r' => Some("\\r"),
            '\t' => Some("\\t"),
            _ => None,
        };
        let must_escape = short.is_some() && c != '/' || (c as u32) < 0x20;
        match (short, must_escape || random.below(3) == 0) {
            (_, false) => out.push(c),
            (Some(short), true) if random.below(2) == 0 => out.push_str(short),
            _ => {
                let mut units = [0; 2];
                c.encode_utf16(&mut units)
                    .iter()
                    .for_each(|unit| match random.below(2) {
                        0 => out.push_str(&format!("\\u{unit:04x}")),
                        _ => out.push_str(&format!("\\u{unit:04X}")),
                    });
            }
        }
    }
    out.push('"');
}

fn write(random: &mut Random, json: &Json, out: &mut String) {
    match json {
        Json::Null => out.push_str("null"),
        Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Json::Number(text) => out.push_str(text),
        Json::String(text) => write_string(random, text, out),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(space(random));
                write(random, item, out);
                out.push_str(space(random));
            }
            out.push(']');
        }
        Json::Object(members) => {
            out.push('{');
            for (index, member) in members.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(space(random));
                write_string(random, &member.key, out);
                out.push_str(space(random));
                out.push(':');
                out.push_str(space(random));
                write(random, &member.value, out);
                out.push_str(space(random));
            }
            out.push('}');
        }
    }
}

/// Reads a JSON value back from the tree.
fn read(node: &Node) -> Json {
    match &node.kind {
        NodeKind::Number(text) => Json::Number(text.clone()),
        NodeKind::String(literal) => Json::String(literal.value.clone()),
        NodeKind::Brackets(items) => Json::Array(items.iter().map(read).collect()),
        NodeKind::Braces(members) => Json::Object(
            members
                .iter()
                .map(|member| match &member.kind {
                    NodeKind::Call(call) if call.name_text() == "_ _" => {
                        let NodeKind::String(key) = &call.arguments[0].kind else {
                            panic!("expected a string key, got {member:?}")
                        };
                        Member {
                            key: key.value.clone(),
                            value: read(&call.arguments[1]),
                        }
                    }
                    _ => panic!("expected a member, got {member:?}"),
                })
                .collect(),
        ),
        NodeKind::Call(call) => match (call.name_text().as_str(), &call.arguments[..]) {
            ("null", []) => Json::Null,
            ("true", []) => Json::Bool(true),
            ("false", []) => Json::Bool(false),
            ("- _", [argument]) => match &argument.kind {
                NodeKind::Number(text) => Json::Number(format!("-{text}")),
                _ => panic!("expected a number after `-`, got {node:?}"),
            },
            _ => panic!("expected a JSON value, got {node:?}"),
        },
        _ => panic!("expected a JSON value, got {node:?}"),
    }
}

#[test]
fn generated_json_reads_back_as_the_same_value() {
    let mut random = Random(0x5EED_1234_ABCD_0042);
    for _ in 0..2000 {
        let json = value(&mut random, 4);
        let mut source = String::new();
        source.push_str(space(&mut random));
        write(&mut random, &json, &mut source);
        source.push_str(space(&mut random));
        let nodes = nodes(&source);
        assert_eq!(nodes.len(), 1, "{source}");
        assert_eq!(read(&nodes[0]), json, "{source}");
    }
}

#[test]
fn formatted_json_reads_back_as_the_same_value() {
    let mut random = Random(0x5EED_1234_ABCD_0042);
    for _ in 0..500 {
        let json = value(&mut random, 4);
        let mut source = String::new();
        write(&mut random, &json, &mut source);
        let formatted = anyfix_expr::format(&source).unwrap();
        let nodes = nodes(&formatted);
        assert_eq!(nodes.len(), 1, "{formatted}");
        assert_eq!(read(&nodes[0]), json, "{formatted}");
        assert_eq!(anyfix_expr::format(&formatted).unwrap(), formatted);
    }
}

#[test]
fn a_leading_byte_order_mark_is_skipped() {
    let nodes = nodes("\u{FEFF}{\"a\": [1e-5]}");
    assert_eq!(
        read(&nodes[0]),
        Json::Object(vec![Member {
            key: "a".into(),
            value: Json::Array(vec![Json::Number("1e-5".into())]),
        }])
    );
}
