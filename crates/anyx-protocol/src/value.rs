//! Values as the schema sees them, between the wire and an app's own types.

use std::collections::BTreeMap;

/// A value of some schema type. The codec checks values against the schema
/// both ways, so a `Value` that came out of it fits its type.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Integer(i64),
    Number(f64),
    Boolean(bool),
    List(Vec<Value>),
    /// A record's fields by name. An `OPTIONAL` field left out is absent.
    Record(BTreeMap<String, Value>),
    /// A choice's variant or an error: its shape's text, such as
    /// `conflict at revision _`, and a value for each slot.
    Variant {
        shape: String,
        arguments: Vec<Value>,
    },
}

/// Turns an app's type into a [`Value`]. The generator implements it for
/// every record and choice of a schema.
pub trait IntoValue {
    fn into_value(self) -> Value;
}

/// Reads an app's type out of a [`Value`]. `None` means the value does not
/// have the type's shape, which a value the codec checked never does.
pub trait FromValue: Sized {
    fn from_value(value: Value) -> Option<Self>;
}

impl IntoValue for String {
    fn into_value(self) -> Value {
        Value::Text(self)
    }
}

impl FromValue for String {
    fn from_value(value: Value) -> Option<Self> {
        match value {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }
}

impl IntoValue for i64 {
    fn into_value(self) -> Value {
        Value::Integer(self)
    }
}

impl FromValue for i64 {
    fn from_value(value: Value) -> Option<Self> {
        match value {
            Value::Integer(integer) => Some(integer),
            _ => None,
        }
    }
}

impl IntoValue for f64 {
    fn into_value(self) -> Value {
        Value::Number(self)
    }
}

impl FromValue for f64 {
    fn from_value(value: Value) -> Option<Self> {
        match value {
            Value::Number(number) => Some(number),
            Value::Integer(integer) => Some(integer as f64),
            _ => None,
        }
    }
}

impl IntoValue for bool {
    fn into_value(self) -> Value {
        Value::Boolean(self)
    }
}

impl FromValue for bool {
    fn from_value(value: Value) -> Option<Self> {
        match value {
            Value::Boolean(boolean) => Some(boolean),
            _ => None,
        }
    }
}

impl<T: IntoValue> IntoValue for Vec<T> {
    fn into_value(self) -> Value {
        Value::List(self.into_iter().map(IntoValue::into_value).collect())
    }
}

impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(value: Value) -> Option<Self> {
        match value {
            Value::List(items) => items.into_iter().map(T::from_value).collect(),
            _ => None,
        }
    }
}

/// Fields, for generated code to build a record or a variant from.
#[derive(Default)]
pub struct FieldsBuilder(BTreeMap<String, Value>);

impl FieldsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field(mut self, name: &str, value: impl IntoValue) -> Self {
        self.0.insert(name.to_string(), value.into_value());
        self
    }

    /// Adds an `OPTIONAL` field when it has a value.
    pub fn optional(mut self, name: &str, value: Option<impl IntoValue>) -> Self {
        if let Some(value) = value {
            self.0.insert(name.to_string(), value.into_value());
        }
        self
    }

    pub fn build(self) -> BTreeMap<String, Value> {
        self.0
    }
}

/// Fields, for generated code to read a record or a variant from.
pub struct FieldsReader(BTreeMap<String, Value>);

impl FieldsReader {
    pub fn new(fields: BTreeMap<String, Value>) -> Self {
        FieldsReader(fields)
    }

    pub fn field<T: FromValue>(&mut self, name: &str) -> Option<T> {
        T::from_value(self.0.remove(name)?)
    }

    /// Reads an `OPTIONAL` field: `Some(None)` when it was left out.
    pub fn optional<T: FromValue>(&mut self, name: &str) -> Option<Option<T>> {
        match self.0.remove(name) {
            None => Some(None),
            Some(value) => T::from_value(value).map(Some),
        }
    }
}
