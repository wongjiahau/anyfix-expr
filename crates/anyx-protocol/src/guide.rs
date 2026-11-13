//! The guide a server sends to a client that has never seen the protocol,
//! such as an AI agent: how to write a request and read a response, then the
//! schema as written.

use crate::schema::{Method, Schema, Type, TypeKind, scoped};

/// The guide to `schema`. `how_to_send` tells the reader where and how to
/// send a request, such as the URL and the header to send it with; when it
/// is empty, as in the answer to `SCHEMA`, the guide leaves it out.
pub fn guide(schema: &Schema, how_to_send: &str) -> String {
    let widest = schema
        .methods
        .iter()
        .max_by_key(|method| method.shape.parameters.len());
    let example = widest.map_or(String::new(), |method| sample_call(schema, method));
    let scoped_example = match widest.filter(|method| !method.scope.is_empty()) {
        Some(_) => "\nA method in a scope is called by its full name, as above: the scope's\npath, `/`, then the call.\n".to_string(),
        None => schema
            .methods
            .iter()
            .find(|method| !method.scope.is_empty())
            .map_or(String::new(), |method| {
                format!(
                    "\nA method in a scope is called by its full name: the scope's path, `/`,\nthen the call:\n\n    {}\n",
                    sample_call(schema, method)
                )
            }),
    };
    format!(
        "# {name} (anyx protocol, version {version})

{how_to_send}## Requests

A request is a list of calls separated by commas. Each call is written in the
shape of one of the methods below, with a value in each `(name: TYPE)` slot:

    {example}
{scoped_example}
Every call runs, in order, and the response has one result per call.

`SCHEMA` is a call every server answers, with this guide.

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

The response has one result per call, in order. Each result repeats its
call, with text over 32 characters and lists over 3 items cut short with
`…`, then one of:

- `RETURNS` and the value the method returns.
- `ERROR` and one of the method's errors, in the error's shape. They follow
  `ERRORS`: its own in brackets, then those of each group joined with `+`.
  For example: `read \"a\" ERROR missing`. The call didn't do anything.
  Each error's `#` lines say what to do next.
- `INTERNAL ERROR`: the server failed. Calls after it don't run.
- `NOT RUN`: the call didn't run, because an earlier call failed that way.

A request that can't be read gets one result, and none of its calls run:

    INVALID REQUEST {{ line 1, column 6, message \"…\" }}

## The schema

In the schema, a `#` line before a definition says what it means. Lowercase
words are names; all-caps words belong to the schema language. `ERRORS name
[…]` is a group of errors that methods share by joining it with `+`.{scopes}

{source}
",
        name = schema.name,
        version = schema.version,
        how_to_send = match how_to_send.is_empty() {
            true => String::new(),
            false => format!("{how_to_send}\n\n"),
        },
        scopes = match schema.files.len() {
            1 => "",
            _ => SCOPES,
        },
        source = sources(schema),
    )
}

const SCOPES: &str = "

The schema is a file per scope, each shown below under its full name. A
type or an error group is private to its scope unless it is `PUBLIC`; another
scope writes its full path, such as `common/diagnostic`. A response writes
an error in its shape alone, without its scope: `ERROR missing`.";

/// The schema's files as written: a schema of one file as it is, else each
/// file under a heading of its scope.
fn sources(schema: &Schema) -> String {
    if let [file] = schema.files.as_slice() {
        return file.source.trim_end().to_string();
    }
    schema
        .files
        .iter()
        .map(|file| {
            let methods: Vec<String> = schema
                .methods
                .iter()
                .filter(|method| method.scope == file.scope)
                .map(|method| format!("`{}`", method.full_name()))
                .collect();
            let heading = match file.scope.is_empty() {
                true => format!("### `{}`", file.path),
                false => format!("### `{}` (`{}`)", file.scope.join("/"), file.path),
            };
            let calls = match methods.is_empty() {
                true => String::new(),
                false => format!("Methods: {}.\n\n", methods.join(", ")),
            };
            format!("{heading}\n\n{calls}{}", file.source.trim_end())
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A call of `method` by its full name, with a sample value in each slot.
fn sample_call(schema: &Schema, method: &Method) -> String {
    let mut values = method
        .shape
        .parameters
        .iter()
        .map(|parameter| sample(schema, &parameter.r#type));
    let call = method
        .shape
        .text
        .split(' ')
        .map(|word| match word {
            "_" => values.next().unwrap_or_default(),
            word => word.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ");
    scoped(&method.scope, &call)
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
