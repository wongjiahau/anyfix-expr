# TODO: the anyx parser generator

A parser generator whose grammar language is anyx, described in itself.
It reads a grammar and writes Rust: a typed AST, plus a decoder that turns
the tree from `SyntaxTree::nodes()` into that AST.

## 1. Workspace

- [x] Move today's crate to `crates/anyx`, with a root `Cargo.toml` workspace
- [x] Create `crates/anyx-generator` (library + command line)
- [x] Keep `cargo test`, `cargo clippy` and lefthook working from the root

## 2. The grammar language

- [x] Put the grammar of grammars in `crates/anyx-generator/grammar.anyx`
- [x] Put the JSON grammar in `crates/anyx-generator/tests/fixtures/json.anyx`
- [x] Write the grammar language reference (README), covering:
  - [x] `ENTRY` first, then rules: `name: pattern`, lowercase names
  - [x] Rule references: bare lowercase words
  - [x] `ONE OF [...]`: first match wins
  - [x] `ZERO OR MORE:`, `ONE OR MORE:`, `OPTIONAL:`
  - [x] `CURLY BRACE`, `SQUARE BRACKET`, `PARENTHESIS`, with `: pattern` or `[elements in order]`
  - [x] `ANYFIX CALL [WORD "IF", condition: expression, ...]`
  - [x] `ANYFIX CALL [NAME [...], ARGUMENTS: pattern]`
  - [x] `WORD "x"`, `ANY WORD [KEYWORD | NAME | SYMBOL | string conditions]`
  - [x] `STRING`, `STRING "exact"`, `STRING [STARTS WITH, ENDS WITH, CONTAINS, MATCHING REGEX]`
  - [x] `NUMBER`, `NUMBER 2`, `NUMBER [INTEGER, AT LEAST n, AT MOST n]`
  - [x] Names: `name: pattern` names a field or a choice. A rule reference defaults to the rule's name, and `WORD "null"` defaults to `Null`

## 3. Handwritten decoder for the grammar language

- [x] Typed AST for `grammar.anyx`, with a span on every node
- [x] Decoder from `anyx::Node` to that AST
- [x] Decode errors with spans: "expected `ONE OF [...]`, found ..."
- [x] Tests: `grammar.anyx` and `json.anyx` decode, and bad grammars give good errors

## 4. Checking a grammar

- [x] Every referenced rule exists, and every rule is reachable from `ENTRY`
- [x] Every built-in part has a name (`key: STRING`), and no two fields or choices share a name
- [x] Names are valid as Rust identifiers once converted
- [x] Warn when two `ONE OF` choices can match the same input
- [x] Reject left recursion that can't terminate (a rule that only refers to itself)

## 5. Generating Rust

- [x] Struct per sequence or call, enum per `ONE OF`, `Vec` for repetition, `Option` for `OPTIONAL`
- [x] Spans everywhere: `span: Span` on structs, `span()` on enums, `Spanned<T>` for built-ins
- [x] `STRING` → `Spanned<String>`, keeping the `StringLiteral` pieces
- [x] `NUMBER` → type chosen by its conditions (`u8`, `i64`, `f64`), with errors when out of range
- [x] `ANYFIX CALL` without `ARGUMENTS` → raw `anyx::Node` arguments
- [x] Recursive types boxed where needed
- [x] Decoder functions, one per rule, with errors pointing at the source
- [x] Generated code is formatted (rustfmt) and passes clippy

## 6. Command line

- [x] `anyx-generate grammar.anyx > src/parser.rs`
- [x] Errors and warnings show line:column and the source line
- [x] Output is deterministic, so committed files diff cleanly

## 7. Bootstrapping

- [x] Generate the grammar language's decoder from `grammar.anyx`
- [x] Replace the handwritten decoder (step 3) with the generated one
- [x] Test: regenerating from `grammar.anyx` gives the committed file byte for byte

## 8. Examples and tests

- [x] JSON: decode `tests/fixtures/json.anyx` (the anyx crate's) into the generated AST
- [x] A keyword language example (`IF _ THEN _ ELSE _`, user-defined calls via `ANYFIX CALL`)
- [x] Error snapshot tests for decode failures

## 9. Skill for AI agents

- [x] A SKILL that teaches an agent to write a grammar and run the generator
      (`skills/anyx-grammar/SKILL.md`), with `anyx-generate --tree` to
      see how anyx groups a sample before writing rules for it
- [x] Worked examples: JSON, a config format, a keyword language
- [x] Common mistakes and how to read the generator's warnings

## Related changes in anyx

- [x] Negative number literals (`-` before a digit always starts a number)
- [x] Every number has a value (`1mb` is `1 mb`)
- [ ] One-line raw strings, for regexes ([#2](https://github.com/wongjiahau/anyx/issues/2))

## Protocol clients and batching

Typed batches replace `batch(build)`, whose result is one type for every
position. A batch is for a few calls written out by hand; a method whose
result grows with the data is the schema's job, not the batch's.

- [ ] Call values: a function per method builds a typed call, such as
      `files.read(id)` giving `Call<File, ReadError>`. It sends nothing;
      `client.call(c)` sends one and `client.batch([...])` sends several.
      `client.files.read(id)` stays as shorthand for
      `client.call(files.read(id))`. TS and Rust generators both.
- [ ] Typed tuple batch of 1 to 5 calls, returning the matching tuple of
      outcomes, each position its own type. Rust: a trait implemented for
      tuples up to 5. TS: a tuple type of length at most 5. Replaces
      `batch(build)`.
- [ ] A Rust client next to the server: call values, `call`, `batch`, and a
      `Codec` plus `send` like the TS client.
- [ ] Server limit on calls per request (5, matching the client cap): a
      request over it is `INVALID REQUEST` before any call runs. Document it
      in `PROTOCOL.md`.

## Open questions

- [x] Decode a list's elements each on its own, keeping the errors (`EACH:`)
- [ ] A way to say "any node" (no wildcard yet)
- [ ] More conditions for `ANYFIX CALL` and `ANY WORD` as needs come up
- [ ] A `build.rs` library entry point, later
- [ ] Output in languages other than Rust, later
