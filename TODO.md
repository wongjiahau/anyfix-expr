# TODO: the anyfix parser generator

A parser generator whose grammar language is anyfix, described in itself.
It reads a grammar and writes Rust: a typed AST, plus a decoder that turns
the tree from `SyntaxTree::nodes()` into that AST.

## 1. Workspace

- [x] Move today's crate to `crates/anyfix-expr`, with a root `Cargo.toml` workspace
- [x] Create `crates/anyfix-generator` (library + command line)
- [x] Keep `cargo test`, `cargo clippy` and lefthook working from the root

## 2. The grammar language

- [x] Put the grammar of grammars in `crates/anyfix-generator/grammar.anyfix`
- [x] Put the JSON grammar in `crates/anyfix-generator/tests/fixtures/json.anyfix`
- [x] Write the grammar language reference (README), covering:
  - [x] `ENTRY` first, then rules: `name: pattern`, lowercase names
  - [x] Rule references: bare lowercase words
  - [x] `ONE OF [...]`: first match wins
  - [x] `ZERO OR MORE:`, `ONE OR MORE:`, `OPTIONAL:`
  - [x] `CURLY BRACE`, `SQUARE BRACKET`, `PARENTHESIS`, with `: pattern` or `[elements in order]`
  - [x] `SPECIFIC ANYFIX CALL [WORD "IF", condition: expression, ...]`
  - [x] `ANYFIX CALL [NAME [...], ARGUMENTS: pattern]`
  - [x] `WORD "x"`, `ANY WORD [KEYWORD | NAME | SYMBOL | string conditions]`
  - [x] `STRING`, `STRING "exact"`, `STRING [STARTS WITH, ENDS WITH, CONTAINS, MATCHING REGEX]`
  - [x] `NUMBER`, `NUMBER 2`, `NUMBER [INTEGER, AT LEAST n, AT MOST n]`
  - [x] Names: `name: pattern` names a field or a choice. A rule reference defaults to the rule's name, and `WORD "null"` defaults to `Null`

## 3. Handwritten decoder for the grammar language

- [x] Typed AST for `grammar.anyfix`, with a span on every node
- [x] Decoder from `anyfix_expr::Node` to that AST
- [x] Decode errors with spans: "expected `ONE OF [...]`, found ..."
- [x] Tests: `grammar.anyfix` and `json.anyfix` decode, and bad grammars give good errors

## 4. Checking a grammar

- [ ] Every referenced rule exists, and every rule is reachable from `ENTRY`
- [ ] Every built-in part has a name (`key: STRING`), and no two fields or choices share a name
- [ ] Names are valid as Rust identifiers once converted
- [ ] Warn when two `ONE OF` choices can match the same input
- [ ] Reject left recursion that can't terminate (a rule that only refers to itself)

## 5. Generating Rust

- [ ] Struct per sequence or call, enum per `ONE OF`, `Vec` for repetition, `Option` for `OPTIONAL`
- [ ] Spans everywhere: `span: Span` on structs, `span()` on enums, `Spanned<T>` for built-ins
- [ ] `STRING` → `Spanned<String>`, keeping the `StringLiteral` pieces
- [ ] `NUMBER` → type chosen by its conditions (`u8`, `i64`, `f64`), with errors when out of range
- [ ] `ANYFIX CALL` without `ARGUMENTS` → raw `anyfix_expr::Node` arguments
- [ ] Recursive types boxed where needed
- [ ] Decoder functions, one per rule, with errors pointing at the source
- [ ] Generated code is formatted (rustfmt) and passes clippy

## 6. Command line

- [ ] `anyfix-generate grammar.anyfix > src/parser.rs`
- [ ] Errors and warnings show line:column and the source line
- [ ] Output is deterministic, so committed files diff cleanly

## 7. Bootstrapping

- [ ] Generate the grammar language's decoder from `grammar.anyfix`
- [ ] Replace the handwritten decoder (step 3) with the generated one
- [ ] Test: regenerating from `grammar.anyfix` gives the committed file byte for byte

## 8. Examples and tests

- [ ] JSON: decode `tests/fixtures/json.anyfix` (the anyfix crate's) into the generated AST
- [ ] A keyword language example (`IF _ THEN _ ELSE _`, user-defined calls via `ANYFIX CALL`)
- [ ] Error snapshot tests for decode failures

## 9. Skill for AI agents

- [ ] A SKILL that teaches an agent to write a grammar and run the generator
- [ ] Worked examples: JSON, a config format, a keyword language
- [ ] Common mistakes and how to read the generator's warnings

## Related changes in anyfix-expr

- [x] Negative number literals (`-` before a digit always starts a number)
- [x] Every number has a value (`1mb` is `1 mb`)
- [ ] One-line raw strings, for regexes ([#2](https://github.com/wongjiahau/anyfix-expr/issues/2))

## Open questions

- [ ] A way to say "any node" (no wildcard yet)
- [ ] More conditions for `ANYFIX CALL` and `ANY WORD` as needs come up
- [ ] A `build.rs` library entry point, later
- [ ] Output in languages other than Rust, later
