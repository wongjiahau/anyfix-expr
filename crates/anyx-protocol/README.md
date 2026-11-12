# anyx-protocol

An RPC protocol written in anyx. A schema declares the methods; requests and
responses are anyx documents, which people and AI agents read and write
easily, checked against the schema.

```rust
let schema = anyx_protocol::Schema::parse(source)?;
let calls = anyx_protocol::decode_request(&schema, request)?;
// Run each call, then:
let response = anyx_protocol::encode_response(&schema, &methods, &outcomes)?;
```

`guide(schema, how_to_send)` is the markdown to hand an agent first. See
[`PROTOCOL.md`](PROTOCOL.md) for the schema language and the wire format.
