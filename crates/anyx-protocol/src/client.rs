//! What a generated client needs: sending one call and reading its outcome.

use std::future::Future;

use crate::codec::{Call, EncodeError, Outcome, Problem, Response, encode_request};
use crate::response::decode_response;
use crate::schema::Schema;
use crate::value::{FromValue, Value};

/// Sends a request to the server and returns its response, such as over
/// HTTP.
pub trait Transport {
    type Error;

    fn send(&self, request: String) -> impl Future<Output = Result<String, Self::Error>>;
}

/// Why a call returned no value.
#[derive(Clone, Debug, PartialEq)]
pub enum CallError<E, T> {
    /// One of the method's errors.
    Method(E),
    /// The server failed. It logged why.
    Internal,
    /// An argument has no form on the wire, such as a `NUMBER` that is not
    /// finite.
    Encode(EncodeError),
    /// The server couldn't read the request.
    InvalidRequest(Problem),
    Transport(T),
    /// The response doesn't fit the schema.
    Response(Problem),
}

/// Calls the method at `method` in [`Schema::methods`] and reads its value.
/// A method without errors uses [`std::convert::Infallible`] for `E`.
pub async fn call<T: Transport, R: FromValue, E: FromValue>(
    transport: &T,
    schema: &Schema,
    method: usize,
    arguments: Vec<Value>,
) -> Result<R, CallError<E, T::Error>> {
    let request =
        encode_request(schema, &[Call { method, arguments }]).map_err(CallError::Encode)?;
    let response = transport
        .send(request)
        .await
        .map_err(CallError::Transport)?;
    let mismatch = || {
        CallError::Response(Problem {
            line: 1,
            column: 1,
            message: "found a value that doesn't fit the generated types; expected the schema they were generated from".to_string(),
        })
    };
    match decode_response(schema, &[method], &response).map_err(CallError::Response)? {
        Response::InvalidRequest(problem) => Err(CallError::InvalidRequest(problem)),
        Response::Outcomes(mut outcomes) => match outcomes.pop() {
            Some(Outcome::Value(value)) => R::from_value(value).ok_or_else(mismatch),
            Some(Outcome::Error(error)) => Err(CallError::Method(
                E::from_value(error).ok_or_else(mismatch)?,
            )),
            Some(Outcome::NotRun | Outcome::InternalError) | None => Err(CallError::Internal),
        },
    }
}
