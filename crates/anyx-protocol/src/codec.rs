//! Reads and writes requests and responses, checking each value against the
//! schema.
//!
//! A request is an anyx document of calls, each written in its method's
//! shape: `read "Hymns/a.poentaro"`. A response holds one outcome per call, in
//! order: the method's value, `ERROR` and one of its errors, `NOT RUN` or
//! `INTERNAL ERROR`. A request that can't be read gets the single response
//! `INVALID REQUEST { … }` instead.

use std::collections::BTreeMap;

use anyx::{Node, NodeKind, Span, line_column};

use crate::schema::{Field, Schema, Shape, Type, TypeKind};
use crate::value::Value;

/// One call of a request: which method, and its arguments in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    /// The index of the method in [`Schema::methods`].
    pub method: usize,
    pub arguments: Vec<Value>,
}

/// What came of one call.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// What the method returns.
    Value(Value),
    /// One of the method's errors, as a [`Value::Variant`].
    Error(Value),
    /// An earlier call hit an internal error, so this one never ran.
    NotRun,
    /// The server failed. Nothing about the failure is sent; the server logs it.
    InternalError,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Response {
    /// One outcome per call, in order.
    Outcomes(Vec<Outcome>),
    /// The request couldn't be read, so nothing ran.
    InvalidRequest(Problem),
}

/// Where a document went wrong, counted from 1 (the column in characters).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for Problem {}

/// A value that has no form on the wire, such as a `NUMBER` that is not
/// finite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodeError {
    pub message: String,
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EncodeError {}

struct Mismatch {
    span: Span,
    message: String,
}

fn problem(source: &str, span: Span, message: String) -> Problem {
    let position = line_column(source, span.start);
    Problem {
        line: position.line,
        column: position.column,
        message,
    }
}

fn parse(source: &str) -> Result<Vec<Node>, Problem> {
    let parsed = anyx::parse(source);
    match parsed.errors.into_iter().next() {
        Some(error) => Err(problem(source, error.span, error.message)),
        None => Ok(parsed.syntax.nodes()),
    }
}

fn quoted_shapes<'a>(shapes: impl Iterator<Item = &'a Shape>) -> String {
    shapes
        .map(|shape| format!("`{}`", shape.text))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Reads a request: every call must name a method and fit its parameters.
pub fn decode_request(schema: &Schema, source: &str) -> Result<Vec<Call>, Problem> {
    let nodes = parse(source)?;
    if nodes.is_empty() {
        return Err(problem(
            source,
            Span::empty(0),
            format!(
                "found an empty request; expected at least one call, such as `{}`",
                schema
                    .methods
                    .first()
                    .map_or("a method", |method| method.shape.text.as_str())
            ),
        ));
    }
    let shapes: Vec<&Shape> = schema.methods.iter().map(|method| &method.shape).collect();
    nodes
        .iter()
        .map(|node| {
            let (method, arguments) = decode_shape(schema, &shapes, node).map_err(|mismatch| {
                let expected = || {
                    format!(
                        "expected one of the methods {}",
                        quoted_shapes(shapes.iter().copied())
                    )
                };
                match mismatch {
                    ShapeMismatch::NoShape(found) => Mismatch {
                        span: node.span,
                        message: format!(
                            "found {found}; {}; a word in a slot, such as a choice's variant or `true`, goes in parentheses",
                            expected()
                        ),
                    },
                    ShapeMismatch::Argument(mismatch) => mismatch,
                }
            })?;
            Ok(Call { method, arguments })
        })
        .collect::<Result<Vec<_>, Mismatch>>()
        .map_err(|mismatch| problem(source, mismatch.span, mismatch.message))
}

enum ShapeMismatch {
    /// The node is no call of any of the shapes; holds what it is.
    NoShape(String),
    Argument(Mismatch),
}

/// Reads `node` as a call of one of `shapes`: which one, and its arguments.
fn decode_shape(
    schema: &Schema,
    shapes: &[&Shape],
    node: &Node,
) -> Result<(usize, Vec<Value>), ShapeMismatch> {
    let NodeKind::Call(call) = &node.kind else {
        return Err(ShapeMismatch::NoShape(describe(node)));
    };
    let text = call.name_text();
    let Some(index) = shapes.iter().position(|shape| shape.text == text) else {
        return Err(ShapeMismatch::NoShape(format!("the call `{text}`")));
    };
    let arguments = shapes[index]
        .parameters
        .iter()
        .zip(&call.arguments)
        .map(|(parameter, argument)| {
            decode(schema, &parameter.r#type, argument).map_err(|mismatch| Mismatch {
                message: format!("in `{}` of `{text}`: {}", parameter.name, mismatch.message),
                ..mismatch
            })
        })
        .collect::<Result<_, _>>()
        .map_err(ShapeMismatch::Argument)?;
    Ok((index, arguments))
}

fn describe(node: &Node) -> String {
    match &node.kind {
        NodeKind::Number(text) => format!("the number `{text}`"),
        NodeKind::String(_) => "a string".to_string(),
        NodeKind::Call(call) => format!("the call `{}`", call.name_text()),
        NodeKind::Parens(_) => "parentheses".to_string(),
        NodeKind::Brackets(_) => "a list `[…]`".to_string(),
        NodeKind::Braces(_) => "braces `{…}`".to_string(),
        NodeKind::Underscore => "`_`".to_string(),
        NodeKind::Missing => "nothing".to_string(),
    }
}

/// Reads `node` as a value of `type`.
fn decode(schema: &Schema, r#type: &Type, node: &Node) -> Result<Value, Mismatch> {
    let mismatch = |expected: String| Mismatch {
        span: node.span,
        message: format!("found {}; expected {expected}", describe(node)),
    };
    match (r#type, &node.kind) {
        (Type::Text, NodeKind::String(literal)) => Ok(Value::Text(literal.value.clone())),
        (Type::Text, _) => Err(mismatch("a string, as in `\"text\"`".to_string())),
        (Type::Bytes, NodeKind::String(literal)) => crate::bytes::decode(&literal.value)
            .map(Value::Bytes)
            .ok_or_else(|| mismatch("base64 in the string, as in `\"TVRoZA==\"`".to_string())),
        (Type::Bytes, _) => Err(mismatch(
            "base64 in a string, as in `\"TVRoZA==\"`".to_string(),
        )),
        (Type::Integer, NodeKind::Number(text)) => text
            .parse()
            .map(Value::Integer)
            .map_err(|_| mismatch("a whole number that fits in 64 bits".to_string())),
        (Type::Integer, _) => Err(mismatch("a whole number, as in `3`".to_string())),
        (Type::Number, NodeKind::Number(text)) => text
            .parse()
            .map(Value::Number)
            .map_err(|_| mismatch("a number".to_string())),
        (Type::Number, _) => Err(mismatch("a number, as in `1.5`".to_string())),
        (Type::Boolean, NodeKind::Call(call)) if call.arguments.is_empty() => {
            match call.name_text().as_str() {
                "true" => Ok(Value::Boolean(true)),
                "false" => Ok(Value::Boolean(false)),
                _ => Err(mismatch("`true` or `false`".to_string())),
            }
        }
        (Type::Boolean, _) => Err(mismatch("`true` or `false`".to_string())),
        (Type::List(item), NodeKind::Brackets(items)) => items
            .iter()
            .map(|node| decode(schema, item, node))
            .collect::<Result<_, _>>()
            .map(Value::List),
        (Type::List(item), _) => Err(mismatch(format!(
            "a list of {}, as in `[…]`",
            schema.type_name(item)
        ))),
        (Type::Named(index), _) => {
            let definition = &schema.types[*index];
            match &definition.kind {
                TypeKind::Record(fields) => match &node.kind {
                    NodeKind::Braces(nodes) => {
                        decode_fields(schema, fields, nodes, node.span).map(Value::Record)
                    }
                    _ => Err(mismatch(format!(
                        "the record `{}`, as in `{{ {} }}`",
                        definition.name,
                        example_fields(fields)
                    ))),
                },
                TypeKind::Choice(variants) => {
                    let shapes: Vec<&Shape> =
                        variants.iter().map(|variant| &variant.shape).collect();
                    decode_variant(
                        schema,
                        &shapes,
                        node,
                        &format!("a variant of `{}`", definition.name),
                    )
                }
            }
        }
    }
}

/// Reads `node` as one of `shapes`, written as a variant: `missing`,
/// `conflict at revision 4`.
fn decode_variant(
    schema: &Schema,
    shapes: &[&Shape],
    node: &Node,
    what: &str,
) -> Result<Value, Mismatch> {
    match decode_shape(schema, shapes, node) {
        Ok((index, arguments)) => Ok(Value::Variant {
            shape: shapes[index].text.clone(),
            arguments,
        }),
        Err(ShapeMismatch::NoShape(found)) => Err(Mismatch {
            span: node.span,
            message: format!(
                "found {found}; expected {what}: {}",
                quoted_shapes(shapes.iter().copied())
            ),
        }),
        Err(ShapeMismatch::Argument(mismatch)) => Err(mismatch),
    }
}

fn example_fields(fields: &[Field]) -> String {
    fields
        .iter()
        .map(|field| format!("{} …", field.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn decode_fields(
    schema: &Schema,
    fields: &[Field],
    nodes: &[Node],
    span: Span,
) -> Result<BTreeMap<String, Value>, Mismatch> {
    let mut values = BTreeMap::new();
    let names = || {
        fields
            .iter()
            .map(|field| format!("`{}`", field.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    for node in nodes {
        let NodeKind::Call(call) = &node.kind else {
            return Err(Mismatch {
                span: node.span,
                message: format!(
                    "found {}; expected a field written as its name and value, as in `{} …`",
                    describe(node),
                    fields.first().map_or("name", |field| field.name.as_str())
                ),
            });
        };
        let name = call.name_text();
        let (Some(words), [value]) = (name.strip_suffix(" _"), &call.arguments[..]) else {
            return Err(Mismatch {
                span: node.span,
                message: format!(
                    "found the call `{name}`; expected a field written as its name and value, as in `{} …`; put a `:` after the name when the value is a word, as in `status: done`",
                    fields.first().map_or("name", |field| field.name.as_str())
                ),
            });
        };
        let Some(field) = fields.iter().find(|field| field.name == words) else {
            return Err(Mismatch {
                span: node.span,
                message: format!("found the field `{words}`; expected one of {}", names()),
            });
        };
        if values.contains_key(words) {
            return Err(Mismatch {
                span: node.span,
                message: format!("found the field `{words}` twice; expected each field once"),
            });
        }
        let value = decode(schema, &field.r#type, value).map_err(|mismatch| Mismatch {
            message: format!("in `{words}`: {}", mismatch.message),
            ..mismatch
        })?;
        values.insert(words.to_string(), value);
    }
    let missing: Vec<String> = fields
        .iter()
        .filter(|field| !field.optional && !values.contains_key(&field.name))
        .map(|field| format!("`{}`", field.name))
        .collect();
    if !missing.is_empty() {
        return Err(Mismatch {
            span,
            message: format!(
                "found no {}; expected every field that is not `OPTIONAL`",
                missing.join(", ")
            ),
        });
    }
    Ok(values)
}

/// Writes calls as a request, laid out the way `anyx::format` lays it out.
pub fn encode_request(schema: &Schema, calls: &[Call]) -> Result<String, EncodeError> {
    let mut out = String::new();
    for call in calls {
        write_shape(
            schema,
            &schema.methods[call.method].shape,
            &call.arguments,
            &mut out,
        )?;
        out.push_str(",\n");
    }
    Ok(layout(out))
}

/// Writes a response to calls of `methods`, one outcome each.
pub fn encode_response(
    schema: &Schema,
    methods: &[usize],
    response: &Response,
) -> Result<String, EncodeError> {
    let mut out = String::new();
    match response {
        Response::InvalidRequest(problem) => {
            out.push_str("INVALID REQUEST { line ");
            out.push_str(&problem.line.to_string());
            out.push_str(", column ");
            out.push_str(&problem.column.to_string());
            out.push_str(", message ");
            write_text(&problem.message, &mut out);
            out.push_str(" }\n");
        }
        Response::Outcomes(outcomes) => {
            for (method, outcome) in methods.iter().zip(outcomes) {
                let method = &schema.methods[*method];
                match outcome {
                    Outcome::Value(value) => {
                        write(schema, &method.returns.r#type, value, &mut out)?
                    }
                    Outcome::Error(error) => {
                        out.push_str("ERROR ");
                        let Value::Variant { shape, arguments } = error else {
                            return Err(EncodeError {
                                message: "found an error that is not a variant".to_string(),
                            });
                        };
                        let Some(error) = schema
                            .errors_of(method)
                            .into_iter()
                            .find(|error| error.shape.text == *shape)
                        else {
                            return Err(EncodeError {
                                message: format!(
                                    "found the error `{shape}`; expected an error of `{}`",
                                    method.shape.text
                                ),
                            });
                        };
                        write_shape(schema, &error.shape, arguments, &mut out)?;
                    }
                    Outcome::NotRun => out.push_str("NOT RUN"),
                    Outcome::InternalError => out.push_str("INTERNAL ERROR"),
                }
                out.push_str(",\n");
            }
        }
    }
    Ok(layout(out))
}

/// Reads a response to calls of `methods`.
pub fn decode_response(
    schema: &Schema,
    methods: &[usize],
    source: &str,
) -> Result<Response, Problem> {
    let nodes = parse(source)?;
    if let [node] = &nodes[..]
        && let NodeKind::Call(call) = &node.kind
        && call.name_text() == "INVALID REQUEST _"
    {
        return invalid_request(&call.arguments[0])
            .map(Response::InvalidRequest)
            .ok_or_else(|| {
                problem(
                    source,
                    node.span,
                    "found an `INVALID REQUEST` without a line, column and message".to_string(),
                )
            });
    }
    if nodes.len() != methods.len() {
        return Err(problem(
            source,
            Span::empty(0),
            format!(
                "found {} outcomes; expected one per call, {}",
                nodes.len(),
                methods.len()
            ),
        ));
    }
    methods
        .iter()
        .zip(&nodes)
        .map(|(method, node)| {
            let method = &schema.methods[*method];
            let result = match &node.kind {
                NodeKind::Call(call) => match call.name_text().as_str() {
                    "NOT RUN" => return Ok(Outcome::NotRun),
                    "INTERNAL ERROR" => return Ok(Outcome::InternalError),
                    "ERROR _" => {
                        let errors = schema.errors_of(method);
                        let shapes: Vec<&Shape> = errors.iter().map(|error| &error.shape).collect();
                        decode_variant(
                            schema,
                            &shapes,
                            &call.arguments[0],
                            &format!("an error of `{}`", method.shape.text),
                        )
                        .map(Outcome::Error)
                    }
                    text => decode(schema, &method.returns.r#type, node)
                        .map(Outcome::Value)
                        .map_err(|mismatch| {
                            match schema
                                .errors_of(method)
                                .iter()
                                .any(|error| error.shape.text == text)
                            {
                                true => Mismatch {
                                    message: format!(
                                        "{}; an error is written after `ERROR`, as in `ERROR {}`",
                                        mismatch.message,
                                        call.name_text().replace(" _", " …")
                                    ),
                                    ..mismatch
                                },
                                false => mismatch,
                            }
                        }),
                },
                _ => decode(schema, &method.returns.r#type, node).map(Outcome::Value),
            };
            result.map_err(|mismatch| problem(source, mismatch.span, mismatch.message))
        })
        .collect::<Result<_, _>>()
        .map(Response::Outcomes)
}

fn invalid_request(node: &Node) -> Option<Problem> {
    let NodeKind::Braces(nodes) = &node.kind else {
        return None;
    };
    let mut line = None;
    let mut column = None;
    let mut message = None;
    for node in nodes {
        let NodeKind::Call(call) = &node.kind else {
            return None;
        };
        match (call.name_text().as_str(), &call.arguments.first()?.kind) {
            ("line _", NodeKind::Number(text)) => line = text.parse().ok(),
            ("column _", NodeKind::Number(text)) => column = text.parse().ok(),
            ("message _", NodeKind::String(literal)) => message = Some(literal.value.clone()),
            _ => return None,
        }
    }
    Some(Problem {
        line: line?,
        column: column?,
        message: message?,
    })
}

/// Lays `text` out canonically. The text is always valid anyx, so the
/// formatter never fails on it; if it did, the text is still a fine message.
fn layout(text: String) -> String {
    anyx::format(&text).unwrap_or(text)
}

/// Writes a call of `shape`: its words, with `arguments` in its slots.
fn write_shape(
    schema: &Schema,
    shape: &Shape,
    arguments: &[Value],
    out: &mut String,
) -> Result<(), EncodeError> {
    if arguments.len() != shape.parameters.len() {
        return Err(EncodeError {
            message: format!(
                "found {} arguments for `{}`; expected {}",
                arguments.len(),
                shape.text,
                shape.parameters.len()
            ),
        });
    }
    let mut slots = shape.parameters.iter().zip(arguments).peekable();
    for (index, word) in shape.text.split(' ').enumerate() {
        if index > 0 {
            out.push(' ');
        }
        match (word, slots.next_if(|_| word == "_")) {
            (_, Some((parameter, value))) => write_slot(schema, &parameter.r#type, value, out)?,
            _ => out.push_str(word),
        }
    }
    Ok(())
}

/// Writes a value that fills a slot: a word there would join the call's
/// name, so a value written as a call goes in parentheses.
fn write_slot(
    schema: &Schema,
    r#type: &Type,
    value: &Value,
    out: &mut String,
) -> Result<(), EncodeError> {
    let call = matches!(value, Value::Boolean(_) | Value::Variant { .. });
    if call {
        out.push('(');
    }
    write(schema, r#type, value, out)?;
    if call {
        out.push(')');
    }
    Ok(())
}

fn write(
    schema: &Schema,
    r#type: &Type,
    value: &Value,
    out: &mut String,
) -> Result<(), EncodeError> {
    match value {
        Value::Text(text) => write_text(text, out),
        Value::Bytes(bytes) => write_text(&crate::bytes::encode(bytes), out),
        Value::Integer(integer) => out.push_str(&integer.to_string()),
        Value::Number(number) if number.is_finite() => out.push_str(&format!("{number:?}")),
        Value::Number(number) => {
            return Err(EncodeError {
                message: format!("found the number {number}; expected a finite number"),
            });
        }
        Value::Boolean(boolean) => out.push_str(if *boolean { "true" } else { "false" }),
        Value::List(items) => {
            let item = match r#type {
                Type::List(item) => item.as_ref(),
                _ => r#type,
            };
            out.push('[');
            for (index, value) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write(schema, item, value, out)?;
            }
            out.push(']');
        }
        Value::Record(values) => {
            let Type::Named(index) = r#type else {
                return Err(mismatched(schema, r#type, "a record"));
            };
            let TypeKind::Record(fields) = &schema.types[*index].kind else {
                return Err(mismatched(schema, r#type, "a record"));
            };
            write_fields(schema, fields, values, out)?;
        }
        Value::Variant { shape, arguments } => {
            let Type::Named(index) = r#type else {
                return Err(mismatched(schema, r#type, "a variant"));
            };
            let TypeKind::Choice(variants) = &schema.types[*index].kind else {
                return Err(mismatched(schema, r#type, "a variant"));
            };
            let Some(variant) = variants.iter().find(|variant| variant.shape.text == *shape) else {
                return Err(EncodeError {
                    message: format!(
                        "found the variant `{shape}`; expected a variant of `{}`",
                        schema.type_name(r#type)
                    ),
                });
            };
            write_shape(schema, &variant.shape, arguments, out)?;
        }
    }
    Ok(())
}

fn mismatched(schema: &Schema, r#type: &Type, found: &str) -> EncodeError {
    EncodeError {
        message: format!("found {found}; expected {}", schema.type_name(r#type)),
    }
}

/// Writes fields in the schema's order, each as its name, a `:`, and its
/// value; the layout drops each `:` that a reader doesn't need.
fn write_fields(
    schema: &Schema,
    fields: &[Field],
    values: &BTreeMap<String, Value>,
    out: &mut String,
) -> Result<(), EncodeError> {
    out.push_str("{ ");
    let mut first = true;
    for field in fields {
        let Some(value) = values.get(&field.name) else {
            continue;
        };
        if !first {
            out.push_str(", ");
        }
        first = false;
        out.push_str(&field.name);
        out.push_str(": ");
        write(schema, &field.r#type, value, out)?;
    }
    out.push_str(" }");
    Ok(())
}

/// Writes text as a raw string when it has lines and reads back the same,
/// and as a quoted string otherwise.
fn write_text(text: &str, out: &mut String) {
    if text.contains('\n')
        && let Some(raw) = raw_string(text)
    {
        out.push_str(&raw);
        return;
    }
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `text` as a raw string, with more quotes than any line starts with, or
/// `None` when a raw string can't hold it exactly, such as a line of only
/// spaces.
fn raw_string(text: &str) -> Option<String> {
    let longest = text
        .lines()
        .map(|line| {
            line.trim_start_matches([' ', '\t'])
                .chars()
                .take_while(|&c| c == '"')
                .count()
        })
        .max()
        .unwrap_or(0);
    let quotes = "\"".repeat((longest + 1).max(3));
    let raw = format!("{quotes}\n{text}\n{quotes}");
    let parsed = anyx::parse(&raw);
    match &parsed.syntax.nodes()[..] {
        [
            Node {
                kind: NodeKind::String(literal),
                ..
            },
        ] if parsed.errors.is_empty() && literal.value == text => Some(raw),
        _ => None,
    }
}
