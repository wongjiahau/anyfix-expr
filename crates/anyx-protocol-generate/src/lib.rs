//! Generates the code of an [`anyx_protocol`] schema:
//!
//! - [`rust`]: the schema's types, a `Server` trait with `handle`, and a
//!   `Client`.
//! - [`wasm`]: `#[wasm_bindgen]` functions that encode a call and decode its
//!   response as JSON, for the TypeScript client.
//! - [`typescript`]: the schema's types and a `Client` that calls into the
//!   wasm functions, so encoding and decoding stay in Rust.

mod names;
mod rust;
mod typescript;
mod wasm;

use std::collections::BTreeMap;

use anyx_protocol::Schema;
use anyx_protocol::schema::{Method, TypeKind};

pub use names::{camel_case, pascal_case, snake_case};
pub use rust::rust;
pub use typescript::typescript;
pub use wasm::wasm;

/// Formats generated Rust with `rustfmt`.
pub use anyx_generator::format as format_rust;

/// Why a schema has no code: two of its names become the same name in the
/// generated code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// The enum of a method's errors: `write _ at revision _ content _` has
/// `WriteAtRevisionContentError`.
pub fn error_enum_name(method: &Method) -> String {
    format!("{}Error", pascal_case(&method.shape.text))
}

/// Names the generated code declares or uses, which a schema's types would
/// shadow.
const RESERVED_TYPE_NAMES: [&str; 14] = [
    "Client",
    "Server",
    "Value",
    "String",
    "Vec",
    "Option",
    "Result",
    "Outcome",
    "FieldsBuilder",
    "FieldsReader",
    "FromValue",
    "IntoValue",
    "CallError",
    "Transport",
];

/// Names the Rust and TypeScript clients declare next to the methods.
const RESERVED_METHOD_NAMES: [&str; 5] = ["new", "call", "codec", "send", "constructor"];

/// Checks that every name the code declares in one place is distinct there.
fn check_names(schema: &Schema) -> Result<(), Error> {
    let types = schema
        .types
        .iter()
        .map(|definition| {
            (
                pascal_case(&definition.name),
                format!("the type `{}`", definition.name),
            )
        })
        .chain(
            schema
                .methods
                .iter()
                .filter(|method| !method.errors.is_empty())
                .map(|method| {
                    (
                        error_enum_name(method),
                        format!("the errors of `{}`", method.shape.text),
                    )
                }),
        );
    let reserved = RESERVED_TYPE_NAMES
        .iter()
        .map(|name| (name.to_string(), format!("the generated `{name}`")));
    distinct("type", reserved.chain(types))?;
    let reserved = RESERVED_METHOD_NAMES
        .iter()
        .map(|name| (name.to_string(), format!("the client's `{name}`")));
    distinct(
        "method",
        reserved.chain(schema.methods.iter().map(|method| {
            (
                snake_case(&method.shape.text),
                format!("`{}`", method.shape.text),
            )
        })),
    )?;
    for definition in &schema.types {
        match &definition.kind {
            TypeKind::Record(fields) => distinct(
                "field",
                fields
                    .iter()
                    .map(|field| (snake_case(&field.name), format!("`{}`", field.name))),
            )?,
            TypeKind::Choice(variants) => distinct(
                "variant",
                variants.iter().map(|variant| {
                    (
                        pascal_case(&variant.shape.text),
                        format!("`{}`", variant.shape.text),
                    )
                }),
            )?,
        }
    }
    for method in &schema.methods {
        distinct(
            "error",
            schema.errors_of(method).into_iter().map(|error| {
                (
                    pascal_case(&error.shape.text),
                    format!("`{}`", error.shape.text),
                )
            }),
        )?;
    }
    for shape in schema
        .methods
        .iter()
        .map(|method| &method.shape)
        .chain(schema.errors.iter().map(|error| &error.shape))
        .chain(
            schema
                .types
                .iter()
                .flat_map(|definition| match &definition.kind {
                    TypeKind::Choice(variants) => {
                        variants.iter().map(|variant| &variant.shape).collect()
                    }
                    TypeKind::Record(_) => Vec::new(),
                }),
        )
    {
        distinct(
            "parameter",
            shape.parameters.iter().map(|parameter| {
                (
                    snake_case(&parameter.name),
                    format!("`{}` in `{}`", parameter.name, shape.text),
                )
            }),
        )?;
    }
    Ok(())
}

/// Fails when two of `names` are the same, naming what each came from.
fn distinct(kind: &str, names: impl Iterator<Item = (String, String)>) -> Result<(), Error> {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for (name, from) in names {
        if let Some(earlier) = seen.get(&name) {
            return Err(Error {
                message: format!(
                    "found {from}, which becomes the {kind} `{name}`, like {earlier}; expected names that stay distinct in the generated code, so rename one"
                ),
            });
        }
        seen.insert(name, from);
    }
    Ok(())
}
