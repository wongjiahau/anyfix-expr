//! What a generated server needs: reading a request, turning what a method
//! returned into an outcome, and writing the response.

use crate::codec::{Call, Outcome, Response, decode_request, encode_response};
use crate::schema::Schema;
use crate::value::{FromValue, IntoValue, Value};

/// Reads a request, or returns the `INVALID REQUEST` response to send back.
pub fn read_request(schema: &Schema, request: &str) -> Result<Vec<Call>, String> {
    decode_request(schema, request).map_err(|problem| {
        encode_response(schema, &[], &Response::InvalidRequest(problem))
            .expect("a problem always has a form")
    })
}

/// The next argument of a call. The codec checked the arguments against the
/// method's parameters, so it is there and has the parameter's type.
pub fn argument<T: FromValue>(arguments: &mut impl Iterator<Item = Value>) -> T {
    arguments
        .next()
        .and_then(T::from_value)
        .expect("the codec checked the arguments")
}

/// The outcome of a method that has no errors. `Err` is the server's own
/// failure, which becomes `INTERNAL ERROR`.
pub fn value<T: IntoValue, X>(returned: Result<T, X>) -> Result<Outcome, X> {
    returned.map(|value| Outcome::Value(value.into_value()))
}

/// The outcome of a method with errors.
pub fn result<T: IntoValue, E: IntoValue, X>(
    returned: Result<Result<T, E>, X>,
) -> Result<Outcome, X> {
    returned.map(|result| match result {
        Ok(value) => Outcome::Value(value.into_value()),
        Err(error) => Outcome::Error(error.into_value()),
    })
}

/// The outcome of the built-in `SCHEMA` call: the guide to `schema`,
/// without how to send a request, since the caller just sent one.
pub fn introspect(schema: &Schema) -> Outcome {
    Outcome::Value(Value::Text(crate::guide::guide(schema, "")))
}

/// Writes the response to `calls`. A value with no form on the wire, such as
/// a `NUMBER` that is not finite, turns its outcome into `INTERNAL ERROR`.
pub fn respond(schema: &Schema, calls: &[usize], outcomes: Vec<Outcome>) -> String {
    let outcomes: Vec<Outcome> = calls
        .iter()
        .zip(outcomes)
        .map(|(method, outcome)| {
            let single = Response::Outcomes(vec![outcome]);
            match encode_response(schema, &[*method], &single) {
                Ok(_) => match single {
                    Response::Outcomes(mut outcomes) => outcomes.remove(0),
                    Response::InvalidRequest(_) => unreachable!(),
                },
                Err(_) => Outcome::InternalError,
            }
        })
        .collect();
    encode_response(schema, calls, &Response::Outcomes(outcomes))
        .expect("every outcome was checked to have a form")
}
