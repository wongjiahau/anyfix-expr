//! Values as JSON, for a TypeScript client that calls the codec through wasm.
//!
//! Names are camelCase: the field `file summary` is `fileSummary`. A choice's
//! variant or error is an object whose `kind` is its words, next to its
//! parameters: `{ "kind": "conflictAtRevision", "revision": 4 }`.

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json, json};

use crate::codec::{Call, Outcome, Response, decode_request, encode_request};
use crate::response::{decode_response, encode_response};
use crate::schema::{Field, Schema, Shape, Type, TypeKind};
use crate::value::Value;

/// `file summary` as `fileSummary`.
pub fn camel_case(name: &str) -> String {
    name.split(' ')
        .enumerate()
        .map(|(index, word)| match index {
            0 => word.to_string(),
            _ => {
                let mut chars = word.chars();
                chars
                    .next()
                    .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                    .unwrap_or_default()
            }
        })
        .collect()
}

/// Writes one call of the method `full_name`, such as `files/read _` or
/// `SCHEMA`, as a
/// request. `arguments` is a JSON array with one value per parameter.
pub fn encode_request_json(
    schema: &Schema,
    full_name: &str,
    arguments: &str,
) -> Result<String, String> {
    let arguments: Json = serde_json::from_str(arguments).map_err(|error| error.to_string())?;
    let call = call_from_json(schema, full_name, &arguments)?;
    encode_request(schema, &[call]).map_err(|error| error.message)
}

/// Writes several calls as one request, from a JSON array with one
/// `{ "method": …, "arguments": […] }` per call, in the form
/// [`decode_request_json`] reads them as. The server runs them in order.
pub fn encode_requests_json(schema: &Schema, calls: &str) -> Result<String, String> {
    let calls: Vec<Json> = serde_json::from_str(calls).map_err(|error| error.to_string())?;
    let calls = calls
        .iter()
        .map(|call| {
            let method = call
                .get("method")
                .and_then(Json::as_str)
                .ok_or_else(|| format!("found the call {call}; expected its \"method\""))?;
            call_from_json(schema, method, call.get("arguments").unwrap_or(&Json::Null))
        })
        .collect::<Result<Vec<_>, _>>()?;
    encode_request(schema, &calls).map_err(|error| error.message)
}

fn call_from_json(schema: &Schema, full_name: &str, arguments: &Json) -> Result<Call, String> {
    let index = method_index(schema, full_name)?;
    let method = schema.method_at(index);
    let Json::Array(arguments) = arguments else {
        return Err(format!(
            "found the arguments {arguments} for `{full_name}`; expected an array"
        ));
    };
    if arguments.len() != method.shape.parameters.len() {
        return Err(format!(
            "found {} arguments for `{full_name}`; expected {}",
            arguments.len(),
            method.shape.parameters.len()
        ));
    }
    let arguments = method
        .shape
        .parameters
        .iter()
        .zip(arguments)
        .map(|(parameter, json)| {
            from_json(schema, &parameter.r#type, json)
                .map_err(|message| format!("in `{}`: {message}", parameter.name))
        })
        .collect::<Result<_, _>>()?;
    Ok(Call {
        method: index,
        arguments,
    })
}

/// Reads a request, as a JSON array with one `{ "method": …, "arguments":
/// […] }` per call, `method` being its full name such as `files/read _`.
/// For a tool that watches requests go by, like a test faking a server.
pub fn decode_request_json(schema: &Schema, text: &str) -> Result<String, String> {
    let calls = decode_request(schema, text).map_err(|problem| problem.to_string())?;
    let calls: Vec<Json> = calls
        .iter()
        .map(|call| {
            let method = schema.method_at(call.method);
            json!({
                "method": method.full_name(),
                "arguments": method
                    .shape
                    .parameters
                    .iter()
                    .zip(&call.arguments)
                    .map(|(parameter, value)| to_json(schema, &parameter.r#type, value))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(Json::Array(calls).to_string())
}

/// Writes the response to `request`, from a JSON array with one outcome per
/// call in the form [`decode_response_json`] reads them as. For a test that
/// fakes a server in front of the TypeScript client.
pub fn encode_response_json(
    schema: &Schema,
    request: &str,
    outcomes: &str,
) -> Result<String, String> {
    let calls = decode_request(schema, request).map_err(|problem| problem.to_string())?;
    let outcomes: Vec<Json> = serde_json::from_str(outcomes).map_err(|error| error.to_string())?;
    if outcomes.len() != calls.len() {
        return Err(format!(
            "found {} outcomes for {} calls",
            outcomes.len(),
            calls.len()
        ));
    }
    let outcomes = calls
        .iter()
        .zip(&outcomes)
        .map(|(call, json)| outcome_from_json(schema, call.method, json))
        .collect::<Result<_, _>>()?;
    encode_response(schema, &calls, &Response::Outcomes(outcomes)).map_err(|error| error.message)
}

fn outcome_from_json(schema: &Schema, index: usize, json: &Json) -> Result<Outcome, String> {
    let method = schema.method_at(index);
    match json.get("kind").and_then(Json::as_str) {
        Some("value") => match &method.returns {
            Some(returns) => {
                let value = json.get("value").unwrap_or(&Json::Null);
                from_json(schema, &returns.r#type, value).map(Outcome::Value)
            }
            None => Ok(Outcome::Value(Value::Nothing)),
        },
        Some("error") => {
            let shapes: Vec<&Shape> = schema
                .errors_of(method)
                .iter()
                .map(|error| &error.shape)
                .collect();
            match json.get("error") {
                Some(Json::Object(error)) => {
                    variant_from_json(schema, &shapes, error).map(Outcome::Error)
                }
                _ => Err("found an error outcome without an \"error\" object".to_string()),
            }
        }
        Some("notRun") => Ok(Outcome::NotRun),
        Some("internalError") => Ok(Outcome::InternalError),
        _ => Err(format!(
            "found the outcome {json}; expected its \"kind\": \"value\", \"error\", \"notRun\" or \"internalError\""
        )),
    }
}

/// Reads the response to one call of the method `full_name`, as JSON:
/// `{ "kind": "value", "value": … }`, `{ "kind": "error", "error": … }`,
/// `{ "kind": "notRun" }`,
/// `{ "kind": "internalError" }`, or
/// `{ "kind": "invalidRequest", "line": …, "column": …, "message": … }`.
pub fn decode_response_json(
    schema: &Schema,
    full_name: &str,
    text: &str,
) -> Result<String, String> {
    let index = method_index(schema, full_name)?;
    let outcomes = decode_outcomes(schema, &[index], text)?;
    Ok(outcomes
        .into_iter()
        .next()
        .unwrap_or(json!({ "kind": "internalError" }))
        .to_string())
}

/// Reads the response to a request of several calls, one of each method in
/// `full_names` (a JSON array of full names) in order, as a JSON array with
/// one outcome per call in the form [`decode_response_json`] gives. A
/// response the server couldn't read the request of gives that
/// `invalidRequest` for every call.
pub fn decode_responses_json(
    schema: &Schema,
    full_names: &str,
    text: &str,
) -> Result<String, String> {
    let full_names: Vec<String> =
        serde_json::from_str(full_names).map_err(|error| error.to_string())?;
    let methods = full_names
        .iter()
        .map(|full_name| method_index(schema, full_name))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json::Array(decode_outcomes(schema, &methods, text)?).to_string())
}

fn decode_outcomes(schema: &Schema, methods: &[usize], text: &str) -> Result<Vec<Json>, String> {
    let response = decode_response(schema, methods, text).map_err(|problem| problem.to_string())?;
    Ok(match response {
        Response::InvalidRequest(problem) => {
            let invalid = json!({
                "kind": "invalidRequest",
                "line": problem.line,
                "column": problem.column,
                "message": problem.message,
            });
            methods.iter().map(|_| invalid.clone()).collect()
        }
        Response::Outcomes(outcomes) => {
            let mut outcomes = outcomes.into_iter();
            methods
                .iter()
                .map(|index| outcome_to_json(schema, *index, outcomes.next()))
                .collect()
        }
    })
}

fn outcome_to_json(schema: &Schema, index: usize, outcome: Option<Outcome>) -> Json {
    let method = schema.method_at(index);
    match outcome {
        Some(Outcome::Value(value)) => match &method.returns {
            Some(returns) => json!({
                "kind": "value",
                "value": to_json(schema, &returns.r#type, &value),
            }),
            None => json!({ "kind": "value" }),
        },
        Some(Outcome::Error(Value::Variant { shape, arguments })) => {
            let shapes: Vec<&Shape> = schema
                .errors_of(method)
                .iter()
                .map(|error| &error.shape)
                .collect();
            json!({ "kind": "error", "error": variant_to_json(schema, &shapes, &shape, &arguments) })
        }
        Some(Outcome::NotRun) => json!({ "kind": "notRun" }),
        Some(Outcome::InternalError | Outcome::Error(_)) | None => {
            json!({ "kind": "internalError" })
        }
    }
}

fn method_index(schema: &Schema, full_name: &str) -> Result<usize, String> {
    (0..=schema.introspection())
        .find(|index| schema.method_at(*index).full_name() == full_name)
        .ok_or_else(|| format!("found the method `{full_name}`; expected a method of the schema"))
}

/// `value` as JSON.
pub fn to_json(schema: &Schema, r#type: &Type, value: &Value) -> Json {
    match value {
        Value::Text(text) => Json::String(text.clone()),
        Value::Bytes(bytes) => Json::String(crate::bytes::encode(bytes)),
        Value::Integer(integer) => json!(integer),
        Value::Number(number) => json!(number),
        Value::Boolean(boolean) => Json::Bool(*boolean),
        Value::Nothing => Json::Null,
        Value::List(items) => {
            let item = match r#type {
                Type::List(item) => item.as_ref(),
                other => other,
            };
            Json::Array(
                items
                    .iter()
                    .map(|value| to_json(schema, item, value))
                    .collect(),
            )
        }
        Value::Record(values) => {
            let fields = match r#type {
                Type::Named(index) => match &schema.types[*index].kind {
                    TypeKind::Record(fields) => &fields[..],
                    TypeKind::Choice(_) => &[],
                },
                _ => &[],
            };
            Json::Object(fields_to_json(schema, fields, values))
        }
        Value::Variant { shape, arguments } => {
            let variants = match r#type {
                Type::Named(index) => match &schema.types[*index].kind {
                    TypeKind::Choice(variants) => {
                        variants.iter().map(|variant| &variant.shape).collect()
                    }
                    TypeKind::Record(_) => Vec::new(),
                },
                _ => Vec::new(),
            };
            variant_to_json(schema, &variants, shape, arguments)
        }
    }
}

/// A variant or an error as `{ "kind": "conflictAtRevision", "revision": 4 }`.
pub fn variant_to_json(
    schema: &Schema,
    shapes: &[&Shape],
    shape: &str,
    arguments: &[Value],
) -> Json {
    let mut object = Map::new();
    let Some(shape) = shapes.iter().find(|known| known.text == shape) else {
        return Json::Object(object);
    };
    object.insert("kind".to_string(), Json::String(camel_case(&shape.words())));
    for (parameter, value) in shape.parameters.iter().zip(arguments) {
        object.insert(
            camel_case(&parameter.name),
            to_json(schema, &parameter.r#type, value),
        );
    }
    Json::Object(object)
}

fn fields_to_json(
    schema: &Schema,
    fields: &[Field],
    values: &BTreeMap<String, Value>,
) -> Map<String, Json> {
    fields
        .iter()
        .filter_map(|field| {
            let value = values.get(&field.name)?;
            Some((
                camel_case(&field.name),
                to_json(schema, &field.r#type, value),
            ))
        })
        .collect()
}

/// Reads JSON as a value of `type`, checking it against the schema.
pub fn from_json(schema: &Schema, r#type: &Type, json: &Json) -> Result<Value, String> {
    let mismatch = |expected: &str| Err(format!("found {json}; expected {expected}"));
    match r#type {
        Type::Text => match json {
            Json::String(text) => Ok(Value::Text(text.clone())),
            _ => mismatch("a string"),
        },
        Type::Bytes => match json {
            Json::String(text) => match crate::bytes::decode(text) {
                Some(bytes) => Ok(Value::Bytes(bytes)),
                None => mismatch("base64 in a string"),
            },
            _ => mismatch("base64 in a string"),
        },
        Type::Integer => match json.as_i64() {
            Some(integer) => Ok(Value::Integer(integer)),
            None => mismatch("a whole number"),
        },
        Type::Number => match json.as_f64() {
            Some(number) => Ok(Value::Number(number)),
            None => mismatch("a number"),
        },
        Type::Boolean => match json {
            Json::Bool(boolean) => Ok(Value::Boolean(*boolean)),
            _ => mismatch("true or false"),
        },
        Type::List(item) => match json {
            Json::Array(items) => items
                .iter()
                .map(|json| from_json(schema, item, json))
                .collect::<Result<_, _>>()
                .map(Value::List),
            _ => mismatch("an array"),
        },
        Type::Named(index) => {
            let definition = &schema.types[*index];
            let Json::Object(object) = json else {
                return mismatch(&format!("an object for `{}`", definition.name));
            };
            match &definition.kind {
                TypeKind::Record(fields) => {
                    fields_from_json(schema, fields, object).map(Value::Record)
                }
                TypeKind::Choice(variants) => {
                    let shapes: Vec<&Shape> =
                        variants.iter().map(|variant| &variant.shape).collect();
                    variant_from_json(schema, &shapes, object)
                }
            }
        }
    }
}

/// Reads `{ "kind": …, … }` as one of `shapes`.
pub fn variant_from_json(
    schema: &Schema,
    shapes: &[&Shape],
    object: &Map<String, Json>,
) -> Result<Value, String> {
    let kind = object
        .get("kind")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let Some(shape) = shapes
        .iter()
        .find(|shape| camel_case(&shape.words()) == kind)
    else {
        let kinds: Vec<String> = shapes
            .iter()
            .map(|shape| format!("\"{}\"", camel_case(&shape.words())))
            .collect();
        return Err(format!(
            "found the kind \"{kind}\"; expected one of {}",
            kinds.join(", ")
        ));
    };
    if let Some(unknown) = object.keys().find(|key| {
        *key != "kind"
            && !shape
                .parameters
                .iter()
                .any(|parameter| camel_case(&parameter.name) == **key)
    }) {
        return Err(format!(
            "found the field \"{unknown}\"; expected only the variant's parameters"
        ));
    }
    let arguments = shape
        .parameters
        .iter()
        .map(|parameter| {
            let name = camel_case(&parameter.name);
            let json = object
                .get(&name)
                .ok_or_else(|| format!("found no field \"{name}\""))?;
            from_json(schema, &parameter.r#type, json)
                .map_err(|message| format!("in \"{name}\": {message}"))
        })
        .collect::<Result<_, _>>()?;
    Ok(Value::Variant {
        shape: shape.text.clone(),
        arguments,
    })
}

fn fields_from_json(
    schema: &Schema,
    fields: &[Field],
    object: &Map<String, Json>,
) -> Result<BTreeMap<String, Value>, String> {
    if let Some(unknown) = object
        .keys()
        .find(|key| !fields.iter().any(|field| camel_case(&field.name) == **key))
    {
        return Err(format!(
            "found the field \"{unknown}\"; expected only the schema's fields"
        ));
    }
    let mut values = BTreeMap::new();
    for field in fields {
        match object.get(&camel_case(&field.name)) {
            None | Some(Json::Null) if field.optional => {}
            None => return Err(format!("found no field \"{}\"", camel_case(&field.name))),
            Some(json) => {
                let value = from_json(schema, &field.r#type, json)
                    .map_err(|message| format!("in \"{}\": {message}", camel_case(&field.name)))?;
                values.insert(field.name.clone(), value);
            }
        }
    }
    Ok(values)
}
