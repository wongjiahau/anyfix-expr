//! Reads and writes responses. Each outcome repeats the call it answers,
//! with long arguments shortened, so a reader can tell which call it
//! answers without counting:
//!
//! ```anyx
//! files/read "a.poentaro" RETURNS { path "a.poentaro", revision 3, content "…" },
//! files/export/midi "b.poentaro" ERROR missing,
//! ```
//!
//! After the call comes `RETURNS` and the method's value, `ERROR` and one
//! of its errors, `INTERNAL ERROR` or `NOT RUN`. Outcomes stay in the order
//! of the calls. A request that can't be read gets the single response
//! `INVALID REQUEST { … }` instead.

use anyx::{NamePart, Node, NodeKind, Span};

use crate::codec::{
    Call, EncodeError, Form, Mismatch, Outcome, Problem, Response, ScopedCall, decode,
    decode_variant, describe, layout, parse, problem, scoped_call, write, write_shape, write_text,
};
use crate::schema::{Method, Schema, Shape, scoped};
use crate::value::Value;

/// Writes the response to `calls`, one outcome each.
pub fn encode_response(
    schema: &Schema,
    calls: &[Call],
    response: &Response,
) -> Result<String, EncodeError> {
    let outcomes = match response {
        Response::InvalidRequest(problem) => {
            let mut out = String::from("INVALID REQUEST { line ");
            out.push_str(&problem.line.to_string());
            out.push_str(", column ");
            out.push_str(&problem.column.to_string());
            out.push_str(", message ");
            write_text(&problem.message, Form::Whole, &mut out);
            out.push_str(" }\n");
            return Ok(layout(out));
        }
        Response::Outcomes(outcomes) => outcomes,
    };
    let written = calls
        .iter()
        .zip(outcomes)
        .map(|(call, outcome)| encode_outcome(schema, call, outcome))
        .collect::<Result<Vec<_>, EncodeError>>()?;
    Ok(match written.as_slice() {
        [outcome] => format!("{outcome}\n"),
        outcomes => outcomes
            .iter()
            .map(|outcome| format!("{outcome},\n"))
            .collect(),
    })
}

/// One outcome after the call it answers, laid out the way `anyx::format`
/// lays it out, after the scope's path, which the formatter would space.
fn encode_outcome(schema: &Schema, call: &Call, outcome: &Outcome) -> Result<String, EncodeError> {
    let method = schema.method_at(call.method);
    let mut out = String::new();
    write_shape(
        schema,
        &method.shape,
        &call.arguments,
        Form::Shortened,
        &mut out,
    )?;
    match outcome {
        Outcome::Value(value) => {
            out.push_str(" RETURNS ");
            write(schema, &method.returns.r#type, value, Form::Whole, &mut out)?;
        }
        Outcome::Error(error) => {
            out.push_str(" ERROR ");
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
                        method.full_name()
                    ),
                });
            };
            write_shape(schema, &error.shape, arguments, Form::Whole, &mut out)?;
        }
        Outcome::NotRun => out.push_str(" NOT RUN"),
        Outcome::InternalError => out.push_str(" INTERNAL ERROR"),
    }
    Ok(scoped(&method.scope, layout(out).trim_end()))
}

/// Reads a response to calls of `methods`. Each outcome must repeat a call
/// of its method; its arguments, which may be shortened, aren't read.
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
            decode_outcome(schema, schema.method_at(*method), node)
                .map_err(|mismatch| problem(source, mismatch.span, mismatch.message))
        })
        .collect::<Result<_, _>>()
        .map(Response::Outcomes)
}

/// What follows the repeated call.
enum Ending<'n> {
    Returns(&'n Node),
    Error(&'n Node),
    NotRun,
    InternalError,
}

/// An outcome split into the call it repeats and what follows it.
struct Split<'n> {
    /// The name before the ending: one slot holding the call, or the words
    /// of a call made of keywords only, such as `SCHEMA`.
    name: &'n [NamePart],
    arguments: &'n [Node],
    ending: Ending<'n>,
}

fn split(node: &Node) -> Option<Split<'_>> {
    let NodeKind::Call(call) = &node.kind else {
        return None;
    };
    let word = |part: &NamePart, expected: &str| matches!(part, NamePart::Word { text, .. } if text == expected);
    match call.name.as_slice() {
        [name @ .., not, run] if word(not, "NOT") && word(run, "RUN") => Some(Split {
            name,
            arguments: &call.arguments,
            ending: Ending::NotRun,
        }),
        [name @ .., internal, error] if word(internal, "INTERNAL") && word(error, "ERROR") => {
            Some(Split {
                name,
                arguments: &call.arguments,
                ending: Ending::InternalError,
            })
        }
        [name @ .., keyword, NamePart::Slot] => {
            let (last, arguments) = call.arguments.split_last()?;
            let ending = match () {
                _ if word(keyword, "RETURNS") => Ending::Returns(last),
                _ if word(keyword, "ERROR") => Ending::Error(last),
                _ => return None,
            };
            Some(Split {
                name,
                arguments,
                ending,
            })
        }
        _ => None,
    }
}

fn decode_outcome(schema: &Schema, method: &Method, node: &Node) -> Result<Outcome, Mismatch> {
    let example = method.full_name().replace(" _", " …");
    let Some(split) = split(node) else {
        return Err(Mismatch {
            span: node.span,
            message: format!(
                "found {}; expected the call it answers, then `RETURNS` and its value, `ERROR` and one of its errors, `INTERNAL ERROR` or `NOT RUN`, as in `{example} RETURNS …`",
                describe(node)
            ),
        });
    };
    if split.name.is_empty() {
        return Err(Mismatch {
            span: node.span,
            message: format!(
                "found an outcome without the call it answers; expected the call first, as in `{example} RETURNS …`"
            ),
        });
    }
    let repeated = repeated_method(&split);
    if repeated.as_deref() != Some(method.full_name().as_str()) {
        let found = repeated.map_or_else(|| "something".to_string(), |name| format!("`{name}`"));
        return Err(Mismatch {
            span: node.span,
            message: format!(
                "found the outcome of {found}; expected the outcome of `{}`, the call in that place of the request",
                method.full_name()
            ),
        });
    }
    match split.ending {
        Ending::NotRun => Ok(Outcome::NotRun),
        Ending::InternalError => Ok(Outcome::InternalError),
        Ending::Error(error) => {
            let errors = schema.errors_of(method);
            let shapes: Vec<&Shape> = errors.iter().map(|error| &error.shape).collect();
            decode_variant(
                schema,
                &shapes,
                error,
                &format!("an error of `{}`", method.full_name()),
            )
            .map(Outcome::Error)
        }
        Ending::Returns(value) => decode(schema, &method.returns.r#type, value)
            .map(Outcome::Value)
            .map_err(|mismatch| match &value.kind {
                NodeKind::Call(call)
                    if schema
                        .errors_of(method)
                        .iter()
                        .any(|error| error.shape.text == call.name_text()) =>
                {
                    Mismatch {
                        message: format!(
                            "{}; an error is written after `ERROR`, as in `{example} ERROR {}`",
                            mismatch.message,
                            call.name_text().replace(" _", " …")
                        ),
                        ..mismatch
                    }
                }
                _ => mismatch,
            }),
    }
}

/// The full name of the method whose call an outcome repeats.
fn repeated_method(split: &Split) -> Option<String> {
    match (split.name, split.arguments) {
        ([NamePart::Slot], [repeated]) => {
            let ScopedCall { scope, call } = scoped_call(repeated).unwrap_or(ScopedCall {
                scope: Vec::new(),
                call: repeated,
            });
            match &call.kind {
                NodeKind::Call(call) => Some(scoped(&scope, &call.name_text())),
                _ => None,
            }
        }
        (words, []) if !words.is_empty() => words
            .iter()
            .map(|part| match part {
                NamePart::Word { text, .. } => Some(text.as_str()),
                NamePart::Slot => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(|words| words.join(" ")),
        _ => None,
    }
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
