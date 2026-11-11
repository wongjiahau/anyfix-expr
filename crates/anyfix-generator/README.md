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
since the later one then never sees it.

### `ZERO OR MORE:`, `ONE OR MORE:`, `OPTIONAL:`

```text
ZERO OR MORE: member
ONE OR MORE: digit
OPTIONAL: label
```

A repetition is written as a prefix, and stands where a sequence of nodes is
described: in `ENTRY`, in a bracket, among the parts of a
`SPECIFIC ANYFIX CALL`, or after `ARGUMENTS:`. It takes as many nodes as
match, and never gives one back: in `[ZERO OR MORE: STRING, STRING]` the
repetition takes every string, so the last `STRING` never matches.

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
```

`ANYFIX CALL` matches any call, which is how a language lets its users
define their own. The AST keeps the call's name text, with `_` for each
slot: `list file in _`.

- `NAME [string conditions]` puts conditions on that name text.
- `ARGUMENTS: pattern` describes the arguments. Without it, they stay raw
  `anyfix_expr::Node`s.

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
