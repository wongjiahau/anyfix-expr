# anyx-protocol-generate

Writes the code of an [anyx-protocol](../anyx-protocol) schema:

```sh
anyx-protocol-generate rust schema.anyx > src/protocol.rs
anyx-protocol-generate wasm schema.anyx > src/protocol_wasm.rs
anyx-protocol-generate typescript schema.anyx > web/src/protocol.ts
```

## Rust

- A struct per `RECORD` and an enum per `CHOICE`, with a variant per shape:
  `custom hue (hue: NUMBER)` is `CustomHue { hue: f64 }`.
- An enum of each method's errors, named after the method:
  `write _ at revision _ content _` has `WriteAtRevisionContentError`.
- A `Server` trait with an `async fn` per method. A method with errors
  returns `Result<Result<T, MethodError>, Self::Error>`; the outer `Err` is
  the server's own failure, which the caller sees as `INTERNAL ERROR`.
- `handle(server, request, on_internal_error)`, which reads a request, runs
  its calls and writes the response.
- A `Client<T: Transport>` with a method per schema method.
- `SCHEMA`, the schema as written, for `anyx_protocol::guide`.

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
```

A schema whose names become the same name in the generated code, such as a
type `read error` next to the method `read _`, is rejected.
