# anyfix-expr

A lexer, parser and lossless tree for **anyfix expressions**: a notation that
only describes grouping and gives nothing a meaning.

## Motivation

Anyfix expressions are to a language what S-expressions are to Lisp and WAT: a
consumer-neutral notation. The crate builds tree structure and nothing else.
Deciding what a word means is the consumer's job.
[resh](https://github.com/wongjiahau/resh) is the first consumer, but this
crate knows nothing about it.

`.`, `:` and `_` are syntactic sugar, like `'` in Lisp: `'x` is read as
`(quote x)` before any evaluator sees it. They fill a slot of a call while
parsing, so the consumer never sees them (see [Sugar](#sugar)).

Consumers read a tree of calls, not flat runs. A run's words name the call
and its other items fill the slots. Keywords (all-caps words) bind loosest,
then symbol words: `x < y < z` is the call `_ < _ < _` of `x`, `y` and `z`,
and `IF x < y THEN x ELSE y` is the call `IF _ THEN _ ELSE _` of `x < y`, `x`
and `y` (see [Tree](#tree)).

## Design principles

1. **The syntax has no meaning.** Tokens and operators only build tree
   structure. No word is special (`true` and `LET` mean nothing here). No
   evaluation, holes, namespaces or types. The one exception is the sugar of
   `.`, `:` and a bare `_`, which is rewritten away and is meaning-free like
   Lisp's `'`.
2. **Letter case only groups.** Words keep the text as written. An all-caps
   word is a *keyword*, which binds looser than other words so that a
   consumer's forms need fewer parentheses (see [Tree](#tree)); it still
   means nothing. Nothing else depends on case: string escapes are
   case-insensitive (`\N` is `\n`, `\U{4a}` is `\u{4A}`).
3. **The word boundary is conventional.** A word is all name characters
   (letters, digits, `_`) or all symbol characters. Switching kinds ends it, so
   `x+1` is three words. There's no kebab-case: multi-word names are runs, like
   `flat map`.
4. **Exactly eleven special characters:** `, : . ( ) [ ] { } # "`. They never
   appear inside a word.
5. **Exactly two operators:** `.` (left-assoc) binds tighter than `:`
   (right-assoc). Both take whole runs as operands, and both are sugar.
   Keywords and symbol words are not operators: they split a run into slots
   but have no precedence or associativity among themselves.
6. **One separator, `,`**, at the top level and inside every bracket. Trailing
   commas are allowed. `;` is an ordinary symbol word.
7. **Newlines are plain whitespace.** They never end anything. The one place
   lines matter is inside a [raw string](#raw-strings), whose lines are its
   content.

## Grammar

```ebnf
(* Structure *)
document   = sequence ;
sequence   = [ expr { "," expr } [ "," ] ] ;     (* top level and inside every bracket *)
expr       = dotted [ ":" expr ] ;                (* ":" right-assoc, loosest *)
dotted     = run { "." run } ;                    (* "." left-assoc *)
run        = item { item } ;                      (* the anyfix run *)
item       = word | number | string
           | "(" sequence ")"
           | "[" sequence "]"
           | "{" sequence "}" ;

(* Lexical *)
special    = "," | ":" | "." | "(" | ")" | "[" | "]" | "{" | "}" | "#" | '"' ;
alnum      = letter | digit | "_" ;
symbol     = ? any char that is not whitespace, special, or alnum ? ;
word       = name | symbols ;
name       = ( letter | "_" ) { alnum } ;
symbols    = symbol { symbol } ;
number     = [ "-" ] ( based | decimal ) ;        (* source text kept *)
decimal    = digits [ "." digits ] [ exponent ] ;
exponent   = ( "e" | "E" ) [ "+" | "-" ] digits ;
based      = "0" ( ( "x" | "X" ) hex { [ "_" ] hex }
                 | ( "b" | "B" ) bit { [ "_" ] bit }
                 | ( "o" | "O" ) octal { [ "_" ] octal } ) ;
digits     = digit { [ "_" ] digit } ;
string     = quoted | raw ;
quoted     = '"' { ? any char except '"' or "\" ? | escape } '"' ;
escape     = "\" ( '"' | "\" | "/" | "b" | "f" | "n" | "t" | "r"
                  | "u{" hex { hex } "}" | "u" hex hex hex hex ) ;
raw        = quotes newline { line newline } indent quotes ;  (* see Raw strings *)
quotes     = '"' '"' '"' { '"' } ;               (* closed by as many as opened *)
comment    = "#" { ? any char except newline ? } ;
whitespace = ? char::is_whitespace ? ;
letter     = ? char::is_alphabetic ? ;
digit      = "0" … "9" ;                          (* ASCII only *)
```

## Tokenization rules

1. Comments and whitespace separate tokens and are kept as trivia. `#` always
   starts a comment, even mid-token: `a#b` is the word `a` followed by a
   comment. A shebang line is just a comment. A byte order mark (U+FEFF) at
   the very start is whitespace.
2. Each special character is its own token, except that `"` starts a string
   and `#` starts a comment.
3. A word is as long as possible within its kind (name or symbols). Switching
   kinds ends it, with or without a space. A `-` before an ASCII digit also
   ends a symbol word, because it starts a number (rule 5).
4. A token starting with an ASCII digit, or with `-` then an ASCII digit, is a
   number. Every number has a value: it is decimal, as in `42`, `1_000`,
   `2.5`, `1e5` and `2.5E-3`, or based, as in `0xFF`, `0b1010` and `0o17`.
   Numbers keep their source text, and reading the value is the consumer's
   job.
   - `.` continues a number only once and only before an ASCII digit: `1.to`
     is `1`, `.`, `to`, and `1.2.3` is `1.2`, `.`, `3`.
   - An exponent (`e` or `E`, an optional sign, then digits) only follows a
     decimal number, so `0xFE-1` is `0xFE` then `-1`. `1e` is `1` then `e`.
   - `_` continues a number only between two digits: `1_` is `1` then `_`.
   - A prefix `0x`, `0b` or `0o` needs a digit of its base after it: `0x` is
     `0` then `x`.
   - Any other letter ends the number: `1mb` is `1` then `mb`, `3days` is
     `3` then `days`, and `0xFFmb` is `0xFF` then `mb`.
5. Negation binds hardest: a `-` directly before an ASCII digit always starts
   a number, whatever comes before it. `x-5` is `x` then `-5`, `5-3` is `5`
   then `-3`, `a<-5` is `a`, `<`, `-5`, and `--5` is `-` then `-5`. A `-`
   followed by anything else is a word: `x - 5`, `- 5` and `x-y` subtract.
6. A name starts with a `char::is_alphabetic` letter or `_`, and continues with
   `char::is_alphanumeric` or `_`. So `x٣` is one name, while `٣` on its own
   (alphanumeric but not alphabetic) is a symbol word.
7. `\u{…}` takes 1 to 6 hex digits and must be a valid Unicode scalar value.
   `\uXXXX` takes exactly 4, as in JSON, and a UTF-16 surrogate pair is two of
   them in a row: `\ud83d\ude00` is 😀. A lone surrogate is invalid. `\/`,
   `\b` and `\f` are JSON's too. Strings may contain literal newlines. No
   interpolation.
8. Three or more `"` at the end of a line open a [raw string](#raw-strings).
   `""` is still the empty string.

## Raw strings

A raw string holds text that is not anyfix, such as an embedded language,
as written:

```text
score """
  bpm=90 key=F4 label="Verse"   # not a comment: this is content
  [S1] 3 3 2_1_
  """,
```

The value is `bpm=90 key=F4 label="Verse"   # not a comment: this is
content` and `[S1] 3 3 2_1_`, on two lines.

- **Opening:** three or more `"`, then the end of the line. Spaces and tabs
  may follow the quotes; anything else is `RawStringOpening`.
- **Closing:** a line holding only spaces or tabs, then exactly as many `"`
  as opened the string. Tokens continue after it, so `""",` closes the string
  and then has a comma.
- **Indentation:** the closing line's spaces and tabs are stripped from every
  content line. Each non-blank line must start with exactly that indentation,
  or it is `RawStringIndentation`. Blank lines are empty.
- **Line breaks:** the one after the opening quotes and the one before the
  closing line are not part of the value. The rest are kept as written, so
  `\r\n` stays `\r\n`.
- **No escapes:** `\`, `"` and `#` are content. A run of quotes is only a
  closer at the start of a line, after the indentation. Content with a line
  starting with `"""` opens with `""""` instead.

A consumer that parses a string's content can point its own errors back into
the file. A `String` node holds a `StringLiteral`: its `value`, and the
`pieces` of the value with the source each came from. `source_offset` maps a
byte offset in the value to one in the source, past stripped indentation and
escapes. An offset inside an escape maps to the escape's start.

## Sugar

`.` and `:` each hand their left value to the run on their other side, as an
argument of the call that run becomes:

- If that run has a bare `_` at its top level, the value fills that slot.
- Otherwise a new slot goes at the front for `.` and at the end for `:`.

| Source | Same as |
| --- | --- |
| `x . y (z)` | `(x) y (z)` |
| `x . y _` | `y (x)` |
| `f : x` | `f (x)` |
| `_ f : x` | `(x) f` |

A `_` inside brackets belongs to those brackets (as in Scala) and is not
consumed. It reaches the consumer as an ordinary `Underscore`. A run that
receives a value may hold at most one bare `_`; a second is `ExtraUnderscore`.

This lets a consumer define infix operators as plain functions:
`1 .+ 1 .+ 2` is `((1) + 1) + 2`, the call `_ + _` of `_ + _` and `2`. There
is no precedence: operators apply left to right. Since negation binds
hardest, infix minus needs a space before a digit: `1 .- 5` is the call
`_ - _` of `1` and `5`, but `1 .-5` hands `1` to the number `-5`.

A consumer should not do recursive descent parsing. `.` and `:` already
provide associative chaining, so the tree arrives fully grouped. The consumer
walks it and matches each call's name against its own signatures. When
designing its syntax, a consumer that wants associative chaining should express
it with `.` (left-assoc) or `:` (right-assoc), not with operators or keywords
of its own that would need precedence parsing. Keywords only fix where a run
splits; they never nest one call in another.

## Tree

Consumers read the abstract view, `SyntaxTree::nodes()`: a document is a list
of nodes, its top-level sequence. It is the tree after the sugar is applied,
with every run read as a call.

```text
Node = Number     (source text)
     | String     (StringLiteral: the decoded value, and where each part came from)
     | Call       { name: [NamePart], arguments: [Node] }
     | Parens     [Node]        // `()`, `(a, b)` or `(a,)`
     | Brackets   [Node]
     | Braces     [Node]
     | Underscore               // a `_` that no `.` or `:` consumed
     | Missing                  // only where an error left a hole

NamePart = Word { text, span }
         | Slot                 // filled by the next argument, in order
```

A run becomes a call in three steps:

1. **Keywords split the run.** A keyword is a name word with an uppercase
   letter and no lowercase one: `IF`, `T` and `X1` are keywords; `If`, `_1`
   and `中` are not. Keywords bind loosest, so the items between them gather
   into one argument each, and each argument then goes through the next two
   steps. `IF x > 0 THEN x ELSE 0 - x` is the call `IF _ THEN _ ELSE _` of
   `x > 0`, `x` and `0 - x`.
2. **Symbol words split the run.** In a run without keywords, symbol words
   (`<=`, `+`, `->`) bind looser than the rest, so the items between them
   gather into one argument each. `my weight <= max (all) < 10` is the call
   `_ <= _ < _` of `my weight`, `max (all)` and `10`.
3. **Words name the call.** Every name word becomes part of the name. Every
   other item (number, string, bracket, `_`, or a value from `.` or `:`) fills
   a slot. `list file in (dir)` is the call `list file in _` of `dir`, and
   `my weight` is the call `my weight` with no arguments.

- Keywords, like symbol words, carry no precedence among themselves: every
  keyword in a run belongs to one call, so a consumer declares the whole
  shape `IF _ THEN _ ELSE _`. A keyword form nested in another needs `:` or
  parentheses, even as the last argument:
  `LET s BE: IF a THEN b ELSE : IF c THEN d ELSE e`. Without the first `:`,
  the run is the one call `LET _ BE IF _ THEN _ ELSE _`.
- A split leaves no slot where nothing stands: `x DONE` is `_ DONE`, and
  `FOR ALL [a]` is `FOR ALL _`.
- A word is a call with no arguments. A run of one non-word item is that item,
  never a call.
- Parens around one element with no comma only group: `(x)` is `x`. The
  exception is `(_)`, which stays `Parens[_]` because the parens mark the
  `_`'s scope.
- `Call::name_text()` joins the name with spaces and writes `_` for each slot:
  `_ <= _ < _`, `list file in _`. A lone `_` is never a word, so the text is
  unambiguous even though words like `x_` may contain `_`.
- A name word mixing symbol and name characters is impossible: `null?` is the
  call `_ ?` of `null`.
- Every node and name word carries a byte `Span { start, end }`. A run's span
  covers all of its source, including parens that only group: `(y)` is the
  node `y` with `y`'s span, but `f (y)` spans the closing `)` too.
  `line_column(source, offset)` converts an offset to a 1-based line and
  column (counted in characters).
- `Missing` stands for an empty operand or element. It appears only when
  `Parsed::errors` is not empty.

## Groupings that must hold

Calls are written as s-expressions with the name first, in backticks when it
has spaces. A call with no arguments is just its name.

| Source | Tree |
| --- | --- |
| `x < y < z` | ``(`_ < _ < _` x y z)`` |
| `my weight <= average (all weights) < max (all weights)` | ``(`_ <= _ < _` `my weight` (`average _` `all weights`) (`max _` `all weights`))`` |
| `IF x > 0 THEN x ELSE 0 - x` | ``(`IF _ THEN _ ELSE _` (`_ > _` x 0) x (`_ - _` 0 x))`` |
| `LET s BE IF a THEN b ELSE c` | ``(`LET _ BE IF _ THEN _ ELSE _` s a b c)`` |
| `LET s BE: IF a THEN b ELSE c` | ``(`LET _ BE _` s (`IF _ THEN _ ELSE _` a b c))`` |
| `RETURNS T` | `` `RETURNS T` `` |
| `a b . c d . e f` | ``(`_ e f` (`_ c d` `a b`))`` |
| `a : b : c` | ``(`a _` (`b _` c))`` |
| `k : x . f` | ``(`k _` (`_ f` x))`` |
| `search { max depth : 3 }` | ``(`search _` Braces[(`max depth _` 3)])`` |
| `x . y _` | ``(`y _` x)`` |
| `_ f : x` | ``(`_ f` x)`` |
| `x . f (_)` | ``(`_ f _` x Parens[_])`` |
| `1 .+ 1 .+ 2` | ``(`_ + _` (`_ + _` 1 1) 2)`` |
| `-5` | `Number "-5"` |
| `x-5` | ``(`x _` -5)`` |
| `x - 5` | ``(`_ - _` x 5)`` |
| `- 5` | ``(`- _` 5)`` |
| `x+1` | ``(`_ + _` x 1)`` |
| `a->b` | ``(`_ -> _` a b)`` |
| `null?` | ``(`_ ?` null)`` |
| `f(x)` | ``(`f _` x)`` |
| `1.to` | ``(`_ to` 1)`` |
| `1.2.3` | ``(`_ _` 1.2 3)`` |
| `3days` | ``(`_ days` 3)`` |
| `rg.search "TODO" in "src"` | ``(`_ search _ in _` rg "TODO" "src")`` |
| `fd.(option) argument` | ``(`_ _ argument` fd option)`` |
| `("rg", x)` | `Parens["rg", x]` |
| `(x)` | `x` |
| `()`, `[]`, `{}` | empty sequences |
| `[a, b,]` | `Brackets[a, b]` |

## Lossless tree

`parse(source).syntax.to_source()` reproduces the input byte for byte, for
every input, valid or not. The concrete tree (`Sequence`, `Element`,
`Expression`, `Run`, `Item`, `Group`, `Token`) keeps every token, commas,
brackets and operators included. Each token has leading trivia (everything
since the previous token's trailing trivia) and trailing trivia (the
whitespace and comment after it on the same line). The document's final trivia
sits on the `End` token. Tokens skipped during recovery are kept as
`Item::Skipped`. The abstract view is derived from this tree.

## Diagnostics

Parsing recovers and reports every error, sorted by position. Each `Error` has
a stable `ErrorCode`, a span, and a one-line message for an agent: what was
found, what was expected, and the fix.

| Code | Example | Span and recovery |
| --- | --- | --- |
| `UnclosedBracket` | `f (a` | The opener. The group ends at the end of input, or before a closer that matches an outer bracket (`[(a]`). |
| `MismatchedBracket` | `(a]` | The closer, which still closes the group. The message names both brackets. |
| `UnexpectedCloser` | `a)` | The closer, which is skipped. |
| `EmptyOperand` | `a..b`, `. a`, `a :`, `: b`, `a::b` | The operator next to the missing run, which becomes `Missing`. |
| `EmptyElement` | `a,,b`, `[, a]` | The comma with nothing before it. A trailing comma is fine. |
| `ExtraUnderscore` | `x . f _ _`, `_ f _ : x` | The second bare `_` in a run that receives a value. It stays an `Underscore`. |
| `UnterminatedString` | `"abc`, a raw string with no closing line | The string, which runs to the end of input. A raw string's value is then its content as written. |
| `InvalidEscape` | `"\q"`, `"\u{110000}"`, `"\uD800"` | The escape, kept as written in the decoded value. |
| `RawStringOpening` | `"""abc` | The text after the opening quotes, which is skipped. The content starts on the next line. |
| `RawStringIndentation` | a content line indented less than the closing line | The line's indentation. The line keeps what follows the indentation it shares with the closing line. |
| `TooDeep` | 1000 nested `(` | The first bracket deeper than `MAX_DEPTH` (256). It and its contents are skipped, so the stack never overflows. |

The notation never reports semantic problems. An unknown word or a `:` in an
odd place is the consumer's job.

## The grammar in anyfix expressions

The grammar written in anyfix expressions
(`crates/anyfix-expr/tests/fixtures/grammar.anyfix`). A grammar consumer
would read each `name _` call as a rule, a `_ | _` call as alternation, any
other call as a sequence of its words and arguments in order, `( )` as
grouping, `[ ]` as optional and `{ }` as repetition. This crate only has to
parse it.

```text
# Anyfix expressions — described in anyfix expressions.

document : sequence,

sequence : [ expr { "," expr } [ "," ] ],
expr     : dotted [ ":" expr ],
dotted   : run { "." run },
run      : item { item },

item     : word | number | string
         | "(" sequence ")"
         | "[" sequence "]"
         | "{" sequence "}",

special    : "," | ":" | "." | "(" | ")" | "[" | "]" | "{" | "}" | "#" | "\"",
alnum      : letter | digit | "_",
symbol     : any char except (whitespace | special | alnum),

word       : name | symbols,
name       : (letter | "_") { alnum },
symbols    : symbol { symbol },

number     : ["-"] (based | decimal),
decimal    : digits ["." digits] [exponent],
exponent   : ("e" | "E") ["+" | "-"] digits,
based      : "0" ( ("x" | "X") hex { ["_"] hex }
                 | ("b" | "B") bit { ["_"] bit }
                 | ("o" | "O") octal { ["_"] octal } ),
digits     : digit { ["_"] digit },

string     : quoted | raw,
quoted     : "\"" { any char except ("\"" | "\\") | escape } "\"",
escape     : "\\" ("\"" | "\\" | "/" | "b" | "f" | "n" | "t" | "r"
                  | "u{" hex { hex } "}" | "u" hex hex hex hex),
raw        : quotes newline { line newline } indent quotes,
quotes     : "\"\"\"" { "\"" },

comment    : "#" { any char except newline },
whitespace : unicode whitespace,

letter     : unicode letter,
digit      : "0" to "9",
```

## JSON

Every JSON document is an anyfix document. Objects are braces of `_ _`
calls (`"key" : value` is the call `_ _` of the key and the value), arrays are
brackets, `true`, `false` and `null` are calls with no arguments, and
numbers (negative ones included) are numbers. JSON's exponents (`1e-5`),
escapes (`\/`, `\b`, `\f`, `\uXXXX` with surrogate pairs) and a leading byte
order mark all parse.

`crates/anyfix-expr/tests/json.rs` generates documents covering every escape, number form and
whitespace character JSON allows, parses each with no errors, and reads its
tree back as the same JSON value. As with any input, folding the tree into
values is the consumer's job; `read` in that test shows how.

## Formatting

`format(source)` returns the document in canonical form, or its parse errors.
The result has the same tree as the input and keeps every comment, and
formatting it again changes nothing.

- **Only redundant syntax goes.** A `.`, `:` or pair of grouping `( )` is
  removed when the tree stays the same without it, and nothing is ever
  rewritten into another form: `1 . f` becomes `1 f`, `f : [a]` becomes
  `f [a]`, `f ((x))` becomes `f (x)` and `a .+ b .+ c` becomes `a + b .+ c`,
  but `x . f`, `f : x` and `f (x)` stay, since `x f` and `f x` are other
  calls. `(x,)` keeps its comma and `(_)` its parens.
- **Layout.** Lines aim for 100 columns, indented by 4 spaces. A bracket that
  doesn't fit puts each element on its own line and ends with a comma, and a
  `.` chain that doesn't fit breaks before each `.`. Top-level elements get
  a line each, with commas when there is more than one.
- **Spacing.** One space between items. `:` hugs its left (`a: b`). `.` has a
  space on both sides (`x . f`), except that it hugs a symbol word after it
  (`1 .+ 2`). Braces pad their contents (`{ a }`); parens and brackets don't.
- **Comments and blank lines.** A comment stays after the token it followed
  on the same line, or on its own line before the next one, moving to a
  neighbour when its token is removed. A trailing comment inside a run ends
  the line, and the run continues one level in. Blank lines collapse to one.
- **Raw strings** are re-indented one level past the line they open on. Their
  values are unchanged.

## Usage

```rust
let parsed = anyfix_expr::parse("search { max depth : 3 }");
assert!(parsed.errors.is_empty());
let nodes = parsed.syntax.nodes();
let anyfix_expr::NodeKind::Call(call) = &nodes[0].kind else { unreachable!() };
assert_eq!(call.name_text(), "search _");
assert_eq!(parsed.syntax.to_source(), "search { max depth : 3 }");
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
