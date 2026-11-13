//! Generates the code of an [`anyx_protocol`] schema:
//!
//! - [`rust`]: the schema's types, a `Server` trait with `handle`, and a
//!   `Client`.
//! - [`wasm`]: `#[wasm_bindgen]` functions that encode a call and decode its
//!   response as JSON, for the TypeScript client.
//! - [`typescript`]: the schema's types and a `Client` that calls into the
//!   wasm functions, so encoding and decoding stay in Rust.

mod names;
pub(crate) mod rust;
mod typescript;
mod wasm;

use std::collections::BTreeMap;

use anyx_protocol::Schema;
use anyx_protocol::schema::{Method, TypeKind, scoped};

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

/// The enum of a method's errors, in the module of its scope:
/// `write _ at revision _ content _` has `WriteAtRevisionContentError`.
pub fn error_enum_name(method: &Method) -> String {
    format!("{}Error", pascal_case(&method.shape.text))
}

/// A name in TypeScript, where every scope shares one namespace, so the
/// scope's words come first: `diagnostic` in `common` is
/// `CommonDiagnostic`.
pub fn scoped_pascal_case(scope: &[String], name: &str) -> String {
    scope
        .iter()
        .map(|segment| pascal_case(segment))
        .collect::<String>()
        + &pascal_case(name)
}

/// The TypeScript union of a method's errors: `FilesReadError`.
pub fn typescript_error_name(method: &Method) -> String {
    format!(
        "{}Error",
        scoped_pascal_case(&method.scope, &method.shape.text)
    )
}

/// The scopes directly in `scope`.
pub fn children(schema: &Schema, scope: &[String]) -> Vec<Vec<String>> {
    schema
        .scopes()
        .into_iter()
        .filter(|child| child.len() == scope.len() + 1 && child.starts_with(scope))
        .collect()
}

/// Whether `scope`, or a scope in it, has a method, so that the client
/// reaches it.
pub fn has_methods(schema: &Schema, scope: &[String]) -> bool {
    schema
        .methods
        .iter()
        .any(|method| method.scope.starts_with(scope))
}

/// The methods declared in `scope` itself.
pub fn methods_in<'s>(schema: &'s Schema, scope: &[String]) -> Vec<(usize, &'s Method)> {
    schema
        .methods
        .iter()
        .enumerate()
        .filter(|(_, method)| method.scope == scope)
        .collect()
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

fn scope_name(scope: &[String]) -> String {
    match scope.is_empty() {
        true => "the root".to_string(),
        false => format!("the scope `{}`", scope.join("/")),
    }
}

/// Checks that every name the code declares in one place is distinct there:
/// in Rust, a module per scope; in TypeScript, one namespace for the types
/// and an object per scope for the methods.
fn check_names(schema: &Schema) -> Result<(), Error> {
    let reserved_types = || {
        RESERVED_TYPE_NAMES
            .iter()
            .map(|name| (name.to_string(), format!("the generated `{name}`")))
    };
    let reserved_methods = || {
        RESERVED_METHOD_NAMES
            .iter()
            .map(|name| (name.to_string(), format!("the client's `{name}`")))
    };
    for scope in schema.scopes() {
        let types = schema
            .types
            .iter()
            .filter(|definition| definition.scope == scope)
            .map(|definition| {
                (
                    pascal_case(&definition.name),
                    format!("the type `{}`", definition.name),
                )
            });
        let error_enums = methods_in(schema, &scope)
            .into_iter()
            .filter(|(_, method)| !method.errors.is_empty())
            .map(|(_, method)| {
                (
                    error_enum_name(method),
                    format!("the errors of `{}`", method.full_name()),
                )
            });
        distinct("type", reserved_types().chain(types).chain(error_enums))?;
        let methods = methods_in(schema, &scope).into_iter().map(|(_, method)| {
            (
                snake_case(&method.shape.text),
                format!("`{}`", method.full_name()),
            )
        });
        let scopes = children(schema, &scope).into_iter().map(|child| {
            (
                snake_case(child.last().expect("a child scope has a name")),
                format!("the scope `{}` in {}", child.join("/"), scope_name(&scope)),
            )
        });
        let introspection = scope.is_empty().then(|| {
            (
                "schema".to_string(),
                "the client's `schema`, which calls `SCHEMA`".to_string(),
            )
        });
        distinct(
            "method",
            reserved_methods()
                .chain(introspection)
                .chain(methods)
                .chain(scopes),
        )?;
        if let Some(segment) = scope
            .iter()
            .find(|segment| ["self", "super", "crate"].contains(&segment.as_str()))
        {
            return Err(Error {
                message: format!(
                    "found the scope `{}`; expected a scope that isn't named `{segment}`, which Rust reserves for module paths",
                    scope.join("/")
                ),
            });
        }
    }
    let typescript_types = schema
        .types
        .iter()
        .map(|definition| {
            (
                scoped_pascal_case(&definition.scope, &definition.name),
                format!("the type `{}`", scoped(&definition.scope, &definition.name)),
            )
        })
        .chain(
            schema
                .methods
                .iter()
                .filter(|method| !method.errors.is_empty())
                .map(|method| {
                    (
                        typescript_error_name(method),
                        format!("the errors of `{}`", method.full_name()),
                    )
                }),
        );
    distinct("TypeScript type", reserved_types().chain(typescript_types))?;
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
        .chain(schema.errors.iter().map(|error| &error.error.shape))
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
