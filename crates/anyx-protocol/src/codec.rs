//! Reads and writes requests, and the values in them and in responses
//! (`response`), checking each value against the schema.
//!
//! A request is an anyx document of calls, each written in its method's
//! shape: `read "Hymns/a.poentaro"`.

use std::collections::BTreeMap;

use anyx::{NamePart, Node, NodeKind, Span, line_column};

use crate::schema::{Field, Schema, Shape, Type, TypeKind, scoped};
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

/// How much of a value to write: all of it, or a shortened form that labels
/// it, which a response uses to repeat the call it answers. Shortened text
/// keeps its first [`SHORTENED_CHARACTERS`] characters and a list its first
/// [`SHORTENED_ITEMS`] items, each followed by `…`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    Whole,
    Shortened,
}

pub(crate) const SHORTENED_CHARACTERS: usize = 32;
pub(crate) const SHORTENED_ITEMS: usize = 3;

pub(crate) struct Mismatch {
    pub(crate) span: Span,
    pub(crate) message: String,
}

pub(crate) fn problem(source: &str, span: Span, message: String) -> Problem {
    let position = line_column(source, span.start);
    Problem {
        line: position.line,
        column: position.column,
        message,
    }
}

pub(crate) fn parse(source: &str) -> Result<Vec<Node>, Problem> {
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
                    .map_or("a method".to_string(), |method| method.full_name())
            ),
        ));
    }
    nodes
        .iter()
        .map(|node| decode_call(schema, node))
        .collect::<Result<Vec<_>, Mismatch>>()
        .map_err(|mismatch| problem(source, mismatch.span, mismatch.message))
}

/// A call in a scope: `files/export/midi "a"` reads as the call `_/_/_` of
/// `files`, `export` and `midi "a"`.
pub(crate) struct ScopedCall<'n> {
    pub(crate) scope: Vec<String>,
    pub(crate) call: &'n Node,
}

/// `node` as a call in a scope, if its name is slots between `/`s and each
/// slot before the last holds a single word.
pub(crate) fn scoped_call(node: &Node) -> Option<ScopedCall<'_>> {
    let NodeKind::Call(call) = &node.kind else {
        return None;
    };
    let slashes_between_slots = call.name.len() >= 3
        && call.name.len() % 2 == 1
        && call
            .name
            .iter()
            .enumerate()
            .all(|(index, part)| match part {
                NamePart::Slot => index % 2 == 0,
                NamePart::Word { text, .. } => index % 2 == 1 && text == "/",
            });
    if !slashes_between_slots {
        return None;
    }
    let (last, segments) = call.arguments.split_last()?;
    let scope = segments
        .iter()
        .map(|segment| match &segment.kind {
            NodeKind::Call(word) if word.arguments.is_empty() => match word.name.as_slice() {
                [NamePart::Word { text, .. }] => Some(text.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(ScopedCall { scope, call: last })
}

/// Reads `node` as a call of one of the schema's methods: in the root, or
/// in the scope its path names.
fn decode_call(schema: &Schema, node: &Node) -> Result<Call, Mismatch> {
    let ScopedCall { scope, call } = scoped_call(node).unwrap_or(ScopedCall {
        scope: Vec::new(),
        call: node,
    });
    let methods: Vec<usize> = (0..=schema.introspection())
        .filter(|index| schema.method_at(*index).scope == scope)
        .collect();
    let shapes: Vec<&Shape> = methods
        .iter()
        .map(|index| &schema.method_at(*index).shape)
        .collect();
    let all_methods = || {
        schema
            .methods
            .iter()
            .map(|method| format!("`{}`", method.full_name()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if methods.is_empty() {
        return Err(Mismatch {
            span: node.span,
            message: format!(
                "found the scope `{}`, which has no methods; expected one of the methods {}",
                scope.join("/"),
                all_methods()
            ),
        });
    }
    let (index, arguments) = decode_shape(schema, &shapes, call).map_err(|mismatch| match mismatch {
        ShapeMismatch::NoShape(found) => {
            let found = match &call.kind {
                NodeKind::Call(call) => format!("the call `{}`", scoped(&scope, &call.name_text())),
                _ => found,
            };
            let elsewhere: Vec<String> = match &call.kind {
                NodeKind::Call(call) => schema
                    .methods
                    .iter()
                    .filter(|method| method.shape.text == call.name_text() && method.scope != scope)
                    .map(|method| format!("`{}`", method.full_name()))
                    .collect(),
                _ => Vec::new(),
            };
            let hint = match elsewhere.is_empty() {
                true => String::new(),
                false => format!("; a method of that shape is in a scope, so write its full name: {}", elsewhere.join(", ")),
            };
            Mismatch {
                span: node.span,
                message: format!(
                    "found {found}; expected one of the methods {}; a word in a slot, such as a choice's variant or `true`, goes in parentheses{hint}",
                    all_methods()
                ),
            }
        }
        ShapeMismatch::Argument(mismatch) => mismatch,
    })?;
    Ok(Call {
        method: methods[index],
        arguments,
    })
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

pub(crate) fn describe(node: &Node) -> String {
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
pub(crate) fn decode(schema: &Schema, r#type: &Type, node: &Node) -> Result<Value, Mismatch> {
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
pub(crate) fn decode_variant(
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

/// Writes calls as a request, each call laid out the way `anyx::format`
/// lays it out, after its scope's path: `files/export/midi "a"`. The
/// formatter would space the `/`s, which reads less like a name.
pub fn encode_request(schema: &Schema, calls: &[Call]) -> Result<String, EncodeError> {
    let written = calls
        .iter()
        .map(|call| {
            let method = schema.method_at(call.method);
            let mut shape = String::new();
            write_shape(
                schema,
                &method.shape,
                &call.arguments,
                Form::Whole,
                &mut shape,
            )?;
            Ok(scoped(&method.scope, layout(shape).trim_end()))
        })
        .collect::<Result<Vec<_>, EncodeError>>()?;
    Ok(match written.as_slice() {
        [call] => format!("{call}\n"),
        calls => calls.iter().map(|call| format!("{call},\n")).collect(),
    })
}

/// Lays `text` out canonically. The text is always valid anyx, so the
/// formatter never fails on it; if it did, the text is still a fine message.
pub(crate) fn layout(text: String) -> String {
    anyx::format(&text).unwrap_or(text)
}

/// Writes a call of `shape`: its words, with `arguments` in its slots.
pub(crate) fn write_shape(
    schema: &Schema,
    shape: &Shape,
    arguments: &[Value],
    form: Form,
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
            (_, Some((parameter, value))) => {
                write_slot(schema, &parameter.r#type, value, form, out)?
            }
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
    form: Form,
    out: &mut String,
) -> Result<(), EncodeError> {
    let call = matches!(value, Value::Boolean(_) | Value::Variant { .. });
    if call {
        out.push('(');
    }
    write(schema, r#type, value, form, out)?;
    if call {
        out.push(')');
    }
    Ok(())
}

pub(crate) fn write(
    schema: &Schema,
    r#type: &Type,
    value: &Value,
    form: Form,
    out: &mut String,
) -> Result<(), EncodeError> {
    match value {
        Value::Text(text) => write_text(text, form, out),
        Value::Bytes(bytes) => write_text(&crate::bytes::encode(bytes), form, out),
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
            let shown = match form {
                Form::Whole => items.len(),
                Form::Shortened => items.len().min(SHORTENED_ITEMS),
            };
            out.push('[');
            for (index, value) in items[..shown].iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write(schema, item, value, form, out)?;
            }
            if shown < items.len() {
                out.push_str(", …");
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
            write_fields(schema, fields, values, form, out)?;
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
            write_shape(schema, &variant.shape, arguments, form, out)?;
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
    form: Form,
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
        write(schema, &field.r#type, value, form, out)?;
    }
    out.push_str(" }");
    Ok(())
}

/// Writes text as a raw string when it has lines and reads back the same,
/// and as a quoted string otherwise. Shortened, it is always quoted, on one
/// line, and text longer than [`SHORTENED_CHARACTERS`] ends in `…`.
pub(crate) fn write_text(text: &str, form: Form, out: &mut String) {
    if form == Form::Whole
        && text.contains('\n')
        && let Some(raw) = raw_string(text)
    {
        out.push_str(&raw);
        return;
    }
    let shown = match form {
        Form::Whole => text.chars().count(),
        Form::Shortened => SHORTENED_CHARACTERS,
    };
    out.push('"');
    for c in text.chars().take(shown) {
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
    if text.chars().nth(shown).is_some() {
        out.push('…');
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
