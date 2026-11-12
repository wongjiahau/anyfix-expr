---
name: anyfix-grammar
description: Write an anyfix grammar (a `.anyfix` file) for a data format or a small language, and run `anyfix-generate` to turn it into a typed Rust parser. Use when asked to parse a config format, a DSL or a keyword language with anyfix, to write or fix a `.anyfix` grammar, or to read the generator's errors and warnings.
---

# Writing an anyfix grammar

An anyfix grammar does not describe characters. anyfix has already parsed
the source into a tree of nodes: numbers, strings, words, brackets and calls.
A grammar says which of those trees are valid, and names their parts. The
generator turns it into Rust: a type for every rule and a decoder from the
tree into those types.

The full reference is `crates/anyfix-generator/README.md`. This skill is the
way of working, the examples and the mistakes to avoid.

## The way of working

1. **Write sample sources first**, the valid ones and a few invalid ones.
2. **Print their trees** and write the grammar from what you see, never from
   what you expect anyfix to do:

   ```sh
   cargo run -p anyfix-generator --bin anyfix-generate -- --tree sample.txt
   ```

   ```text
   the call `server _`
     `{...}`
       the call `port _`
         the number `8080`
       the call `verbose logging`
   ```

   The descriptions are the ones decode errors use. A call's name is its
   words with `_` for each argument, in order.
3. **Write the grammar**: `ENTRY` first, then one rule per idea.
4. **Generate**, and fix every error and warning:

   ```sh
   cargo run -p anyfix-generator --bin anyfix-generate -- grammar.anyfix > src/parser.rs
   ```

   An error stops the output. A warning does not, but each one is a real
   problem in the grammar; leave none.
5. **Decode the samples in a test**, and check the invalid ones give errors
   that point at the right place:

   ```rust
   let parsed = anyfix_expr::parse(source);
   assert!(parsed.errors.is_empty());
   let document = parser::decode_document(&parsed.syntax.nodes()).unwrap();
   ```

   `error.render(source, path)` gives the message with the source line.
6. **Commit the generated file.** It is the same text for the same grammar.
   It needs `anyfix-expr` as a dependency, and `regex` when the grammar uses
   `MATCHING REGEX`. To keep `regex` out (it is large, in wasm especially),
   say it with `NO ARGUMENTS`, `STARTS WITH` or `ENDS WITH` instead.

## How anyfix groups a source

Most surprises come from here. Check with `--tree` when in doubt.

| Source | Tree |
| --- | --- |
| `a, b` at the top | two nodes; **a newline does not separate** |
| `"key": 1` | the call `_ _`: `:` is not a word, it is dropped |
| `port: 8080` | the call `port _` |
| `max depth: 3` | the call `max depth _`: lowercase words are the name |
| `verbose logging` | the call `verbose logging`, with no arguments |
| `print x` | the call `print x`: `x` is a name word, **not an argument** |
| `print (x)`, `print 1` | the call `print _` |
| `LET x = 1` | the call `LET _` of the call `_ = _` of `x` and `1` |
| `IF a THEN b ELSE c` | the call `IF _ THEN _ ELSE _` |
| `x + 1 * 2` | one call `_ + _ * _`, no precedence |
| `(x)` | the node `x`: parentheses only group |
| `(x,)`, `()`, `(a, b)` | parentheses |
| `{ a, b }`, `[a, b]` | braces, square brackets, of nodes split by `,` |

Only literals, brackets, and runs split off by a KEYWORD (all capitals) or a
symbol (`=`, `+`, `<`) become arguments. Two lowercase words side by side are
one name.

## The grammar language in brief

```anyfix
ENTRY [ZERO OR MORE: statement],        # the whole source: a list of nodes
ENTRY: value,                           # or exactly one node

rule: pattern,                          # rules are lowercase
choice: ONE OF [first: pattern, second: pattern],   # first match wins
```

| Pattern | Matches | Rust |
| --- | --- | --- |
| `other rule` (a bare name) | what the rule matches | the rule's type |
| `STRING`, `STRING "x"`, `STRING [STARTS WITH "a"]` | a string | `Spanned<String>` |
| `NUMBER`, `NUMBER [INTEGER, AT LEAST 0, AT MOST 65535]` | a number | `Spanned<u16>`, chosen from the conditions |
| `WORD "null"` | that one word | a unit variant, or nothing in a call |
| `ANY WORD [NAME]` (also `KEYWORD`, `SYMBOL`) | one word | `Spanned<String>` |
| `CURLY BRACE: pattern` / `SQUARE BRACKET: pattern` / `PARENTHESIS: pattern` | a bracket of one element | |
| `CURLY BRACE [a: x, b: y]` | a bracket of these elements in order | a struct |
| `ZERO OR MORE:`, `ONE OR MORE:`, `OPTIONAL:` | inside a bracket, `ENTRY`, a call or `ARGUMENTS:` | `Vec`, `Option` |
| `EACH:` | every element left, each decoded on its own | `Vec<Result<T, DecodeError>>` |
| `SPECIFIC ANYFIX CALL [WORD "IF", condition: x, WORD "THEN", then: y]` | the call `IF _ THEN _` | a struct |
| `ANYFIX CALL [NAME [MATCHING REGEX "^[a-z]+ _$"], ARGUMENTS: x]` | any call whose name fits | a struct with `name: CallName` and `arguments` |
| `ANYFIX CALL [NO ARGUMENTS]` | a call of words alone: `sans serif` | a struct with `name: CallName` |
| `ANY NODE` | any one node, for your own code to decode | `anyfix_expr::Node` |

Every built-in part needs a name: `key: STRING`, not `STRING`. A rule
reference and a `WORD` of letters name themselves. The names become the Rust
names: field `key`, variant `Key`, type `MemberKey`, function `decode_member`.
Spell names out (`parenthesis`, not `parens`).

Regexes are written in a normal string, so each backslash is written twice:
`MATCHING REGEX "^\\d+$"`.

## Worked examples

Each one is in `examples/`, generates with no warnings, and is tested in
`crates/anyfix-generator/tests/generated.rs`.

### JSON (`examples/json.anyfix`)

```anyfix
ENTRY: value,

value: ONE OF [
  object,
  array,
  string: STRING,
  number: NUMBER,
  WORD "null",
  WORD "true",
  WORD "false",
],

object: CURLY BRACE: ZERO OR MORE: member,

# `"key": value` is the call `_ _` of the key and the value.
member: SPECIFIC ANYFIX CALL [key: STRING, value],

array: SQUARE BRACKET: ZERO OR MORE: value,
```

What to notice: `:` disappears, so a member is just two nodes side by side.

### A config format (`examples/config.anyfix`)

For sources like:

```text
server {
  host: "localhost",
  port: 8080,
  allowed origins: ["a", "b"],
  verbose logging,
  tls { certificate: "cert.pem" },
}
```

```anyfix
ENTRY [ZERO OR MORE: section],

# `server { ... }` is the call `server _` of the braces. `EACH` decodes
# every setting on its own, so one mistake does not hide the others.
section: ANYFIX CALL [
  NAME [MATCHING REGEX "^[a-z][a-z0-9 ]* _$"],
  ARGUMENTS: CURLY BRACE: EACH: setting,
],

setting: ONE OF [
  # `port: 8080` is the call `port _` of the value, and so is
  # `tls { ... }`. The key may be several words: `max depth: 3`.
  pair: ANYFIX CALL [
    NAME [MATCHING REGEX "^[a-z][a-z0-9 ]* _$"],
    ARGUMENTS: value,
  ],
  # `verbose logging` alone is a flag: a call of only words.
  flag: ANYFIX CALL [NAME [MATCHING REGEX "^[a-z][a-z0-9 ]*$"]],
],

value: ONE OF [
  text: STRING,
  number: NUMBER,
  list: SQUARE BRACKET: ZERO OR MORE: value,
  section: CURLY BRACE: EACH: setting,
],
```

What to notice: the keys are not known in advance, so they are matched by
`ANYFIX CALL` with a regex over the call's name (`port _`), and the key is
read back from `name`. The `pair` regex ends in ` _$` and the `flag` regex
allows no `_`, so the two never compete. `EACH` keeps a setting that does not fit as its
`DecodeError`, so a config with two typos reports both, and the settings
around them still decode. Use it wherever a user edits a list by hand.
When the keys *are* known, prefer one
`SPECIFIC ANYFIX CALL [WORD "port", port: NUMBER [INTEGER, AT LEAST 1, AT MOST 65535]]`
per key: the types and the errors get better.

### A keyword language (`examples/language.anyfix`)

```anyfix
ENTRY [ZERO OR MORE: statement],

statement: ONE OF [
  let: SPECIFIC ANYFIX CALL [WORD "LET", binding],
  return: SPECIFIC ANYFIX CALL [WORD "RETURN", OPTIONAL: expression],
  expression,
],

# `total = 1` is the call `_ = _` of the name and the value.
binding: SPECIFIC ANYFIX CALL [name: ANY WORD [NAME], WORD "=", value: expression],

expression: ONE OF [
  if: SPECIFIC ANYFIX CALL [
    WORD "IF", condition: expression,
    WORD "THEN", then: expression,
    WORD "ELSE", else: expression,
  ],
  not: SPECIFIC ANYFIX CALL [WORD "NOT", expression],
  less: SPECIFIC ANYFIX CALL [left: expression, WORD "<", right: expression],
  sum: SPECIFIC ANYFIX CALL [left: expression, WORD "+", right: expression],
  variable: ANY WORD [NAME],
  number: NUMBER,
  text: STRING,
  block: CURLY BRACE: ZERO OR MORE: statement,
  # Any other call that starts with a name and takes arguments is the
  # user's own: `print _`, `move _ to _`.
  call: ANYFIX CALL [
    NAME [MATCHING REGEX "^[a-z].* _"],
    ARGUMENTS: ONE OR MORE: expression,
  ],
],
```

What to notice: `LET x = 1` is `LET _` holding `_ = _`, so `binding` is its
own call. Statements are separated by `,`. `x + 1 < 2` is one call
`_ + _ < _`, which neither `sum` nor `less` matches: mixed operators need
parentheses in the source, `(x + 1) < 2`.

## Common mistakes

- **Expecting a newline to separate items.** `a\nb` is one call. Separate
  items with `,`.
- **Expecting `print x` to have an argument.** Lowercase words join into the
  name. Match it with `ANYFIX CALL` and a name regex, or ask for `print (x)`.
- **`PARENTHESIS: expression` for grouping.** `(x)` is just `x`, so leave the
  parentheses out of the grammar; anything may already be written in them.
- **A repetition where one node is matched**, as in
  `ONE OF [items: ZERO OR MORE: value]`. Repeat inside a bracket instead:
  `SQUARE BRACKET: ZERO OR MORE: value`.
- **A general choice before a specific one.** `ONE OF` takes the first match,
  so `ANY WORD` before `WORD "null"` swallows `null`.
- **A regex with one backslash.** `"\d"` is not `\d`; write `"\\d"`.
- **Naming a rule `document`.** `Document` is the type of `ENTRY`.
- **Editing the generated file.** Change the grammar and generate again.

## Reading the generator's messages

Every message has the same shape: what was found; what was expected; how to
fix it, with the line and column. Do what the last part says.

```text
grammar.anyfix:2:30: error: found a reference to `lsit`, which is not a rule; expected the name of a rule of this grammar; write `list`, or define `lsit: ...`
```

The warnings, and what they mean in practice:

- **"found the choices `a` and `b` able to match the same input"**: `b` can
  never win where `a` also fits. Make `b` more specific than `a` (a `WORD`,
  a `STRING "exact"`, a narrower regex) or move it first. Sometimes the fix
  is in the sample instead: the two inputs really were the same tree.
- **"found the rule `x`, which nothing reachable from `ENTRY` refers to"**:
  a typo in a reference elsewhere, usually; otherwise remove the rule.
- **"found a `PARENTHESIS` around exactly one element"**: see grouping above.

Errors that need a design change rather than a typo fix:

- **"referring to itself with nothing matched in between"**: a rule reaches
  itself through `ONE OF` alone, so it would decode the same node forever.
  Put the reference inside a call or a bracket.
- **"a second part of the grammar that becomes the Rust type `X`"**: two
  paths of names give the same type (rule `value text` and rule `value`'s
  choice `text`). Rename one.
- **"a repeated `WORD` among the parts of a call"**: `A _ A _` is not one
  shape to match; write one `SPECIFIC ANYFIX CALL` per shape.

Decode errors, from the generated parser, have the same shape and point at
the source being decoded:

```text
input:1:10: found the call `b _`; expected the call `_ _` or the closing `}`
```

When one surprises you, print the source's tree.
