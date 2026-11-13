---
title: "A # line string above a rule, field or choice is its documentation; a ## comment is dropped"
author: Wongjiahau
date: 2026-11-13
branch: feat/anyx-lsp-poc
id: l-7d3e91a2
scope: repo
status: active
src: agent
touches:
  - crates/anyx-generator/grammar.anyx
  - crates/anyx-generator/src/lower.rs
  - crates/anyx-generator/src/model.rs
  - crates/anyx-generator/src/generate.rs
  - crates/anyx-generator/src/overlap.rs
  - crates/anyx-lsp/src/lib.rs
---

## Decided
- In a grammar, a `#` line string directly above a rule, a field or a choice documents it, for the people who write the language. A `##` comment is for whoever reads the grammar, and is dropped (anyx keeps it as trivia, not a node).
- No new syntax was needed: `# doc` then `name: pattern` already parses as the call `_ name _` with the string as its first argument, so the meta grammar's `named` became `ONE OF [documented, plain]`.
- The documentation becomes `///` on the generated type, variant or field, and a `DOCUMENTATION` table keyed by how errors describe what may be written (``the call `title _` ``, ``the word `title` ``). anyx-lsp looks completions up in it, so the server needs no grammar-specific code.

## Watch out
- At runtime a keyed call like `title "x"` is missed at its first words, so the error says ``the word `title` ``, not ``the call `title _` ``. The table holds both forms.
- Adding the documented form made the overlap check warn that `field: named` and `pattern` (a rule reference) overlap: the AnyCall vs SpecificCall arm treated any `WORDS` as fitting every name. It now builds a regex of the call's name shape (slots stay `_`) and intersects it with the name's conditions.

In the code: `named` in crates/anyx-generator/grammar.anyx; `documentation` in src/lower.rs; `Grammar::documentation` and `describe_missed` in src/model.rs; `doc_comment` and `DOCUMENTATION` in src/generate.rs; `name_shape` in src/overlap.rs; `Language::documentation` in crates/anyx-lsp/src/lib.rs.
