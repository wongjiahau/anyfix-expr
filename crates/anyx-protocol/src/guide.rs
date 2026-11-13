//! The guide a server sends to a client that has never seen the protocol,
//! such as an AI agent: how to write a request and read a response, then the
//! schema as written.

use crate::schema::{Schema, Type, TypeKind};

/// The guide to `schema`. `how_to_send` tells the reader where and how to
/// send a request, such as the URL and the header to send it with.
pub fn guide(schema: &Schema, how_to_send: &str) -> String {
    let example = schema
        .methods
        .iter()
        .max_by_key(|method| method.shape.parameters.len())
        .map_or(String::new(), |method| {
            let mut values = method
                .shape
                .parameters
                .iter()
                .map(|parameter| sample(schema, &parameter.r#type));
            method
                .shape
                .text
                .split(' ')
                .map(|word| match word {
                    "_" => values.next().unwrap_or_default(),
                    word => word.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        });
    format!(
        "# {name} (anyx protocol, version {version})

{how_to_send}

## Requests

A request is a list of calls separated by commas. Each call is written in the
shape of one of the methods below, with a value in each `(name: TYPE)` slot:

    {example}

Every call runs, in order, and the response has one result per call.

## Values

- TEXT is a string: `\"some text\"`. Escape `\\\"` and `\\\\`, and write a line
  break as `\\n`. For text with many lines, use a raw string: three quotes,
  the lines, then three quotes on a line of their own. The closing quotes'
  indentation is removed from every line. A raw string has no escapes.

      \"\"\"
          first line
          second line
          \"\"\"

- BYTES is base64 in a string: `\"TVRoZA==\"`.
- INTEGER is a whole number, NUMBER any number: `3`, `-1.5`.
- BOOLEAN is `true` or `false`.
- `LIST: T` is `[a, b, c]`.
- A RECORD is its fields in braces, each its name and value, in any order:
  `{{ path \"a\", revision 3 }}`. An `OPTIONAL` field may be left out. When a
  value is a word, put `:` after the field's name: `{{ status: done }}`.
- A CHOICE is one of its variants, written in the variant's shape with a
  value in each slot: `dark`, `custom hue 0.5`.
- In a slot, a value written as words goes in parentheses so that it doesn't
  join the words around it: `set (true)`, `pick (custom hue 0.5)`.

## Responses

The response has one result per call, in order. A result is one of:

- The value the method `RETURNS`.
- `ERROR` and one of the errors the method lists after `ERRORS`, in the
  error's shape: `ERROR missing`, `ERROR conflict at revision 4 content \"…\"`.
  The call didn't do anything. Each error's `#` lines say what to do next.
- `INTERNAL ERROR`: the server failed. Calls after it don't run.
- `NOT RUN`: the call didn't run, because an earlier call failed that way.

A request that can't be read gets one result, and none of its calls run:

    INVALID REQUEST {{ line 1, column 6, message \"…\" }}

## The schema

In the schema, a `#` line before a definition says what it means. Lowercase
words are names; all-caps words belong to the schema language. `ERRORS name
[…]` names a list of errors that methods share.

{source}
",
        name = schema.name,
        version = schema.version,
        source = schema.source.trim_end(),
    )
}

/// A value of `type` as an example fills a slot with it.
fn sample(schema: &Schema, r#type: &Type) -> String {
    match r#type {
        Type::Text => "\"a\"".to_string(),
        Type::Bytes => "\"AQI=\"".to_string(),
        Type::Integer => "3".to_string(),
        Type::Number => "1.5".to_string(),
        Type::Boolean => "(true)".to_string(),
        Type::List(_) => "[…]".to_string(),
        Type::Named(index) => match schema.types[*index].kind {
            TypeKind::Record(_) => "{ … }".to_string(),
            TypeKind::Choice(_) => "(…)".to_string(),
        },
    }
}
