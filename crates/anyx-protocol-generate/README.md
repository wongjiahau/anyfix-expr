# anyx-protocol-generate

Writes the code of an [anyx-protocol](../anyx-protocol) schema:

```sh
anyx-protocol-generate rust schema.anyx > src/protocol.rs
anyx-protocol-generate wasm schema.anyx > src/protocol_wasm.rs
anyx-protocol-generate typescript schema.anyx > web/src/protocol.ts
```

The schema is one file, or a folder of scopes (see
[`PROTOCOL.md`](../anyx-protocol/PROTOCOL.md#scopes)).

## Rust

- A struct per `RECORD` and an enum per `CHOICE`, with a variant per shape:
  `custom hue (hue: NUMBER)` is `CustomHue { hue: f64 }`.
- An enum of each method's errors, named after the method:
  `write _ at revision _ content _` has `WriteAtRevisionContentError`.
- A `Server` trait with an `async fn` per method. A method with errors
  returns `Result<Result<T, MethodError>, Self::Error>`; the outer `Err` is
  the server's own failure, which the caller sees as `INTERNAL ERROR`. A
  method that `RETURNS NOTHING` returns `()`, and `void` in TypeScript.
- `handle(server, request, on_internal_error)`, which reads a request, runs
  its calls and writes the response. It answers `SCHEMA` itself.
- A `Client<T: Transport>` with a method per schema method, and `schema()`,
  which calls the built-in `SCHEMA` and returns the guide.
- `SCHEMA_FILES`, the schema as written, and `schema()`, the schema read
  once, for `anyx_protocol::guide`.

Each scope is a module of its own, `files::export`, with its types, the
error enums of its methods, a `Server` trait of its methods and a `Client`.
A scope's `Server` extends the root's, which holds `type Error`, and
`handle` needs every one of them:

```rust
impl poentaro::Server for Agent { type Error = worker::Error; }
impl poentaro::files::Server for Agent {
    async fn read(&self, path: String) -> Result<Result<File, ReadError>, Self::Error> { … }
}
let pdf = client.files().export().pdf(path).await;
```

The module needs `anyx-protocol`.

## wasm and TypeScript

TypeScript only sends the request: encoding and decoding stay in Rust. The
`wasm` output has two `#[wasm_bindgen]` functions, such as
`encodePoentaroRequest` and `decodePoentaroResponse`, for a crate that has
`wasm-bindgen` and `anyx-protocol` with its `json` feature. The `typescript`
output has the types as JSON (camelCase names, and a `kind` on each variant
and error) and a `Client`:

```ts
const client = new Client(
  { encodeRequest: encodePoentaroRequest, decodeResponse: decodePoentaroResponse },
  (request) => fetch(url, { method: "POST", body: request }).then((r) => r.text()),
);
const outcome = await client.read("hymn.poentaro");
const pdf = await client.files.export.pdf("hymn.poentaro"); // a scope's method
// Several calls in one request, run in order:
const [a, b] = await client.batch((calls) => [calls.read("a"), calls.read("b")]);
```

The wasm functions take and give every call of a request at once:
`encodePoentaroRequest('[{"method":"read _","arguments":["a"]}]')` and
`decodePoentaroResponse('["read _"]', response)`, one outcome per call.

In TypeScript every type shares one namespace, so a scoped type's name starts
with its scope: `diagnostic` in `common` is `CommonDiagnostic`, and the errors
of `files/read _` are `FilesReadError`.

A schema whose names become the same name in the generated code, such as a
type `read error` next to the method `read _`, is rejected.
