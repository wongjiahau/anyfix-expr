# anyfix-generator

A parser generator for [anyfix expressions](../../README.md). You describe
your language in a grammar, itself written in anyfix, and the generator
writes Rust: a typed AST, plus a decoder that turns the tree from
`anyfix_expr::SyntaxTree::nodes()` into that AST.

anyfix has already grouped the source before the decoder runs, so a grammar
never describes characters or precedence. It describes shapes of the tree:
which calls, words, strings, numbers and brackets may stand where.

```text
# JSON, as an anyfix grammar.

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

## The grammar language

The grammar language describes itself in [`grammar.anyfix`](grammar.anyfix).

Two rules of thumb cover most of it:

- **ALL-CAPS words are instructions** of the grammar language: `ONE OF`,
  `STRING`, `WORD`. Words of your own language are always strings:
  `WORD "IF"`, `WORD "-"`, `WORD "null"`.
- **Lowercase words are names**: of a rule, a field or a choice.

A pattern matches one node of the tree, unless it is a repetition, which
matches several nodes in a row.

### `ENTRY` and rules

A grammar is `ENTRY` first, then its rules, separated by commas.

```text
ENTRY: value,                       # the document is one value
ENTRY [header, ZERO OR MORE: rule], # the document is a header, then rules
```

`ENTRY` describes the document's top-level sequence: the elements between
its commas. `ENTRY: x` is short for `ENTRY [x]`.

`name: pattern` at the top level defines a rule. A rule's name is one or more
lowercase words (letters and digits, starting with a letter): `value`,
`key value`, `utf8 text`.

### Rule references

A bare lowercase name is a reference to the rule of that name: `value`,
`key value`.

### `ONE OF [...]`

```text
value: ONE OF [object, array, string: STRING, WORD "null"],
```

The choices are tried in the order written, and the first one that matches
wins. The generator warns whenever two choices can match the same input,
since the later one then never sees it, unless the later one matches
everything the earlier one does: a fallback after the cases it catches,
such as `[minus: SPECIFIC ANYFIX CALL [WORD "-", n: NUMBER], other: ANYFIX CALL]`.

### `ZERO OR MORE:`, `ONE OR MORE:`, `OPTIONAL:`, `EACH:`

```text
ZERO OR MORE: member
ONE OR MORE: digit
OPTIONAL: label
EACH: setting
```

A repetition is written as a prefix, and stands where a sequence of nodes is
described: in `ENTRY`, in a bracket, among the parts of a
`SPECIFIC ANYFIX CALL`, or after `ARGUMENTS:`. It takes as many nodes as
match, and never gives one back: in `[ZERO OR MORE: STRING, STRING]` the
repetition takes every string, so the last `STRING` never matches.

`EACH:` is for lists where one mistake should not hide the rest, such as
the settings of a config file or the statements of a program. It takes
every element that is left and decodes each one on its own. An element that
does not fit is kept as its `DecodeError`, in its place, and the elements
after it still decode. So `EACH` comes last in its list, and the list it is
in matches whatever its elements are.

```text
section: SPECIFIC ANYFIX CALL [WORD "server", settings: CURLY BRACE: EACH: setting],
```

### `CURLY BRACE`, `SQUARE BRACKET`, `PARENTHESIS`

```text
CURLY BRACE: ZERO OR MORE: member    # { ... }: any number of members
SQUARE BRACKET [x: NUMBER, y: NUMBER] # [1, 2]: exactly these, in order
PARENTHESIS []                        # (): nothing inside
```

After the bracket's name comes what it holds: `[elements]` in order, or
`: element` as a short form of a list of one. In a grammar, `[...]` is
always a list of the grammar language, never a bracket of your language.

`PARENTHESIS` matches what anyfix keeps as parentheses: `()`, `(a, b)` and
`(a,)`. Parentheses around one element with no comma only group, so `(x)`
is `x` and no `PARENTHESIS` is there to match.

### `SPECIFIC ANYFIX CALL [...]`

```text
SPECIFIC ANYFIX CALL [WORD "IF", condition: expression, WORD "THEN", then: expression]
SPECIFIC ANYFIX CALL [key: STRING, value]      # the call `_ _`
SPECIFIC ANYFIX CALL [WORD "print", ZERO OR MORE: expression]
```

The parts describe one call, in order. A `WORD` part is one of the call's
name words. Every other part fills the call's next slot. A repetition makes
the call variadic: the last example matches `print`, `print _`,
`print _ _` and so on.

### `ANYFIX CALL`

```text
ANYFIX CALL                                             # any call
ANYFIX CALL [NAME [STARTS WITH "get "]]                 # by its name
ANYFIX CALL [ARGUMENTS: ZERO OR MORE: expression]       # with typed arguments
ANYFIX CALL [NAME [ENDS WITH " _"], ARGUMENTS: expression]
ANYFIX CALL [NO ARGUMENTS]                              # words alone: `sans serif`
```

`ANYFIX CALL` matches any call, which is how a language lets its users
define their own. The AST keeps the call's name text, with `_` for each
slot: `list file in _`.

- `NAME [string conditions]` puts conditions on that name text.
- `ARGUMENTS: pattern` describes the arguments. Without it, they stay raw
  `anyfix_expr::Node`s.
- `NO ARGUMENTS` takes only a call with none, which is one or more words
  with no `_` in its name. Unlike a regex on the name, it needs no `regex`
  crate in the output.

A word is a call with no arguments, so `ANYFIX CALL` matches words too.

### `WORD "x"` and `ANY WORD`

```text
WORD "null"                        # exactly this word
ANY WORD                           # any word
ANY WORD [NAME]                    # any word that is not all-caps or a symbol
ANY WORD [SYMBOL, STARTS WITH "<"] # `<`, `<=`, `<-`
```

`WORD "x"` matches the word `x` standing alone: a keyword, a name or a
symbol. `ANY WORD` matches any single word, and takes conditions, which
must all hold:

- `KEYWORD`: an all-caps name, such as `IF`.
- `NAME`: a name that is not all-caps, such as `width`.
- `SYMBOL`: a word of symbol characters, such as `<=`.
- any string condition (see below), on the word's text.

### `STRING`

```text
STRING                                  # any string
STRING "version"                        # exactly this string
STRING [STARTS WITH "v", ENDS WITH "x", CONTAINS "-", MATCHING REGEX "^[a-z]+$"]
```

The conditions are on the string's value, after escapes, and must all hold.
`MATCHING REGEX` takes a [regex](https://docs.rs/regex) and finds it
anywhere in the value, so anchor it with `^` and `$` to match the whole
value. Until one-line raw strings arrive
([#2](https://github.com/wongjiahau/anyfix-expr/issues/2)), write a regex
backslash twice: `"^\\d+$"`.

### `NUMBER`

```text
NUMBER                                    # any number
NUMBER 2                                  # exactly this number
NUMBER [INTEGER, AT LEAST 0, AT MOST 255] # an integer from 0 to 255
```

The conditions are on the number's value, not its text: `0xFF`, `255`,
`2.55e2` and `25_5` are all the integer 255. A number that breaks a
condition is a decode error at that number.

### Names

`name: pattern` at the top level defines a rule. Inside a list it names a
field of the surrounding call or bracket, or a choice of the surrounding
`ONE OF`:

```text
member: SPECIFIC ANYFIX CALL [key: STRING, value],
value: ONE OF [object, text: STRING, WORD "null"],
```

The name can be left out when it is obvious:

- A rule reference is named after its rule: `value` above is the field
  `value`, and `object` is the choice `object`. So is a repetition of one:
  `ZERO OR MORE: member` is the field `member`.
- `WORD "null"` in a `ONE OF` is the choice `null`. Among the parts of a
  `SPECIFIC ANYFIX CALL` or the elements of a bracket, a `WORD` is only
  matched and makes no field.

Anything else must be named: `STRING`, `NUMBER`, a symbol word such as
`WORD "-"`, a nested bracket or call, and the same rule used twice. The
generator reports a missing name, and two fields or choices with the same
name.

## Running the generator

```sh
anyfix-generate grammar.anyfix > src/parser.rs
```

The output is one Rust file, which builds on Rust edition 2021 and later. It
needs `anyfix-expr` as a dependency, and `regex` when the grammar uses
`MATCHING REGEX`. Commit it: the same grammar
always gives the same text, so it only changes when the grammar does.

The file is laid out with `rustfmt`, so `cargo fmt` leaves it alone.
`--unformatted` prints it as generated, where `rustfmt` is not installed.

Errors and warnings about the grammar go to the terminal, with the line
they are about. An error stops the output; a warning does not:

```text
grammar.anyfix:2:30: error: found a reference to `lsit`, which is not a rule; expected the name of a rule of this grammar; write `list`, or define `lsit: ...`
  value: ONE OF [text: STRING, lsit],
                               ^^^^
```

To see what a grammar has to describe, print the tree of a sample source:

```text
$ anyfix-generate --tree sample.conf
the call `server _`
  `{...}`
    the call `port _`
      the number `8080`
    the call `verbose logging`
```

## Using the generated parser

```rust
mod parser;

let parsed = anyfix_expr::parse(source);
// Report `parsed.errors` first: they are about the anyfix syntax itself.
match parser::decode_document(&parsed.syntax.nodes()) {
    Ok(document) => { /* a `parser::Document` */ }
    Err(error) => eprintln!("{}", error.render(source, "input.json")),
}
```

`decode_document` decodes what `ENTRY` describes. Each rule also has a
decoder of its own, `decode_<rule>`, from one node.

### The types

For the JSON grammar at the top, the generator writes:

```rust
pub struct Document { pub value: Value, pub span: Span }

pub enum Value {
    Object(Object),
    Array(Array),
    String(Spanned<StringLiteral>),
    Number(Spanned<f64>),
    Null(Span),
    True(Span),
    False(Span),
}

pub struct Object { pub member: Vec<Member>, pub span: Span }
pub struct Member { pub key: Spanned<StringLiteral>, pub value: Value, pub span: Span }
pub struct Array { pub value: Vec<Value>, pub span: Span }
```

| In the grammar | In Rust |
|---|---|
| `ENTRY` | `struct Document` |
| a rule that is a call or a bracket | a `struct`, with a field for each named part |
| a rule that is a `ONE OF` | an `enum`, with a variant for each choice |
| a rule that is another rule or a built-in | a type alias |
| a choice that is a call | a variant with fields: `If { condition, then, r#else, span }` |
| a choice that is a `WORD` | a variant holding the word's `Span` |
| `ZERO OR MORE:`, `ONE OR MORE:` | `Vec<T>` |
| `EACH:` | `Vec<Result<T, DecodeError>>` |
| `OPTIONAL:` | `Option<T>` |
| `STRING` | `Spanned<StringLiteral>`: the value, and its pieces as written |
| `NUMBER` | `Spanned<f64>`, or an integer type (see below) |
| `ANY WORD` | `Spanned<String>` |
| a named `WORD` | `Span` |
| `ANYFIX CALL` | a struct of `name: Spanned<String>` and `arguments` |
| a bracket that is a field or a choice, around one unnamed value | `Spanned<T>`: the value, and the bracket's span |

A rule `key value` is the type `KeyValue`, decoded by `decode_key_value`.
A call or `ONE OF` written inside a rule gets a type named after the way to
it: the choices of the field `kind` of the rule `entry` are `EntryKind`.

**Spans are everywhere.** Every struct has a `span` field, every enum a
`span()` method, and every built-in value is `Spanned`. A span is a range
of bytes of the source, as in `anyfix_expr`.

**Numbers** get the type their conditions allow. `NUMBER [INTEGER, AT LEAST
0, AT MOST 255]` is a `u8`: with both bounds, an integer gets the smallest
type that holds them. `INTEGER` alone is an `i64`, with only `AT LEAST 0` a
`u64`. Without `INTEGER` a number is an `f64`.

**`ANYFIX CALL`** keeps the name as text with `_` for each argument, such
as `move _ to _`. With `ARGUMENTS: pattern` its `arguments` are decoded:
one value, or a `Vec` or `Option` for a repetition. Without it they stay
`Vec<anyfix_expr::Node>`, for you to decode.

**Recursion** is boxed where a type would otherwise contain itself.

A rule may be named like a type the output uses itself, such as `string`
or `node`. The rule gets the short name, and the output writes the other
by its full path (`std::string::String`, `anyfix_expr::Node`).

### How a node is matched

- The choices of a `ONE OF` are tried in order, and the first that matches
  wins. The generator warns when two choices can match the same input,
  since the later one then never gets it, except when the later one
  matches everything the earlier one does, which makes it a fallback.
- The elements of a list are matched from left to right. A repetition takes
  every node that fits and gives none back, so in `[ZERO OR MORE: value,
  last: value]` nothing is left for `last`. `EACH` takes every node left,
  whether it fits or not.
- A call matches `SPECIFIC ANYFIX CALL` only with exactly its words and
  arguments, and a bracket only when no element is left over.

### Decode errors

A `DecodeError` says what was found and what the grammar allows there:

```text
input.json:1:5: found the word `yes`; expected the word `null`, `true`, `false`, `{...}`, `[...]`, a string, a number or the closing `]`
  [1, yes]
      ^^^
```

It has the `span`, `found` and `expected` as fields, `message()` for the
one line, and `render(source, file)` for the line with its source. When
several choices fail, the error is the one that got furthest into the
source, with every expectation that was possible there.

## What the generator checks

Errors, which stop the generation:

- a reference to a rule that does not exist, and a rule defined twice
- a field or choice with no name, or two with the same name
- a name that cannot be a Rust identifier (`self`, `super`, `crate`), a
  field named `span`, a rule named `document`
- a repetition where exactly one node is matched, such as a whole rule or
  a choice
- anything after `EACH` in its list, which `EACH` would have taken
- conditions nothing can meet, and a regex that does not compile
- a rule that refers to itself with no call or bracket in between

Warnings:

- two choices of a `ONE OF` that can match the same input, unless the
  later one is a fallback that matches everything the earlier one does
- a rule that nothing reachable from `ENTRY` refers to
- a `PARENTHESIS` around exactly one element, which only `(x,)` matches
