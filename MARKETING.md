# anyfix-expr — marketing draft

> Draft copy for a launch post, crates.io/GitHub blurb, and social posts.
> Every technical claim below is taken from the README and tests; check the
> "Before publishing" list at the bottom first.

---

## Taglines (pick one)

- **S-expressions without the parentheses.**
- **Write a language. Skip the parser.**
- **Syntax that reads like English and parses like Lisp.**
- **The notation that means nothing, so your language can mean anything.**

## One-liner

anyfix-expr is a zero-dependency Rust crate that parses a small, readable
notation into a fully grouped tree of calls, so you can build a DSL, config
format or scripting language without writing a parser or debating operator
precedence.

## Elevator pitch (short)

Every new language starts the same way: a lexer, a grammar, a precedence
table, error recovery, and months of bikeshedding over syntax. Lisp skipped all
of that with S-expressions, at the cost of parentheses everywhere.

**Anyfix expressions** give you the same deal with a syntax people actually
want to read:

```text
IF x > 0 THEN x ELSE 0 - x
list file in (dir)
my weight <= average (all weights) < max (all weights)
search { max depth : 3, include hidden file, glob ["*.rs", "!target"] }
```

anyfix-expr turns each line into a call with a readable name and its
arguments: `IF _ THEN _ ELSE _`, `list file in _`, `_ <= _ < _`. Your
language just matches those names against its own signatures. No recursive
descent, no precedence climbing, no grammar file.

---

## Launch post

### Stop writing parsers

Lisp got one thing deeply right: separate *reading* from *meaning*. The
reader turns text into a tree; the language decides what the tree does. That
is why Lisps sprout DSLs so easily.

The price was syntax. `(if (> x 0) x (- 0 x))` is easy for a machine and
hard on people.

**anyfix-expr keeps the split and drops the price.** It is a notation that
only describes grouping. It gives no word a meaning: `true`, `LET` and `+`
are just words. It builds a tree and hands it to you.

### Calls you can read out loud

In anyfix, a call's name can wrap around its arguments: prefix, infix,
postfix, or all at once. That's the "any" in anyfix.

| You write | Your language receives |
| --- | --- |
| `list file in (dir)` | call `list file in _` with `dir` |
| `x < y < z` | call `_ < _ < _` with `x`, `y`, `z` |
| `IF a THEN b ELSE c` | call `IF _ THEN _ ELSE _` with `a`, `b`, `c` |
| `null?` | call `_ ?` with `null` |
| `rg.search "TODO" in "src"` | call `_ search _ in _` with `rg`, `"TODO"`, `"src"` |

Three rules do all the grouping:

1. **ALL-CAPS keywords split a run first**, so `IF … THEN … ELSE …` needs no
   parentheses.
2. **Symbol words split next**, so `my weight <= max (all)` groups the way
   you'd read it.
3. **Name words form the call's name**, everything else fills its slots.

There is no precedence table. `.` chains left to right, `:` chains right to
left, and that is the entire operator story:

```text
1 .+ 1 .+ 2            # ((1) + 1) + 2
output . split on ":" . count
f : g : x              # f (g (x))
```

Define `_ + _` as an ordinary function in your language and you have infix
operators, for free.

### Tiny, and on purpose

- **11 special characters**: `, : . ( ) [ ] { } # "`. Nothing else is magic.
- **2 operators**: `.` and `:`. Both are sugar, rewritten away like Lisp's `'`.
- **1 separator**: `,`. Newlines are just whitespace.
- **0 dependencies.**

You can learn the whole notation in an afternoon. The grammar is short enough
that it is written in anyfix itself (`tests/fixtures/grammar.anyfix`).

### Every JSON file already parses

Every JSON document is a valid anyfix document, escapes, exponents, surrogate
pairs and BOM included. The test suite generates JSON covering every form the
spec allows and reads each one back as the same value. Start with your
existing config files and add comments, trailing commas and readable calls
as you go:

```text
# a config file that grew up
search { max depth : 3, include hidden file, glob ["*.rs", "!target"], }
```

### Built for tooling, not just parsing

- **Lossless.** `parse(src).syntax.to_source()` gives back the input byte for
  byte, for every input, valid or not. Comments and whitespace are kept as
  trivia. Write formatters, refactoring tools and language servers on top of
  it.
- **Never gives up.** Parsing recovers and reports every error, sorted by
  position. Nesting is capped, so hostile input can't overflow the stack.
- **Errors written for humans and agents.** Each diagnostic has a stable
  code, a span, and a one-line message: what was found, what was expected,
  and how to fix it.
- **Spans everywhere.** Every node and name word carries a byte span, with a
  helper for line/column.
- **Embed other languages.** Raw strings (`"""`) hold SQL, regexes, music
  notation or anything else as written, with indentation stripped. Your
  embedded parser's errors can still point to the exact spot in the file.

### Who it's for

- **Language designers** who want to prototype semantics, not syntax.
- **DSL and config authors** who have outgrown JSON/YAML/TOML but don't
  want to ship a custom parser.
- **Tool builders** who need a lossless tree for formatting, linting or
  editor support.
- **Agent and LLM tooling**, where a small, regular syntax and fix-it error
  messages make generated code easier to check and repair.

### Proven in practice

anyfix-expr is the reader behind
[resh](https://github.com/wongjiahau/resh), a readable, typed shell, and the
crate knows nothing about it. Here's resh code wrapping ripgrep, all parsed
by anyfix-expr with no resh-specific rules:

```text
function { (output : String) match : List : Match } {
    output . split on ":" at most 4 . let (parts) be (_),
    unless (parts . count . equals 4) do : { return ([]) },
    new (Match) : { path : parts . item 0, line : line, column : column, text : parts . item 3 }
        . singleton
}
```

### Try it

```toml
[dependencies]
anyfix-expr = { git = "https://github.com/wongjiahau/anyfix-expr" }
```

```rust
let parsed = anyfix_expr::parse("search { max depth : 3 }");
assert!(parsed.errors.is_empty());

let nodes = parsed.syntax.nodes();
let anyfix_expr::NodeKind::Call(call) = &nodes[0].kind else { unreachable!() };
assert_eq!(call.name_text(), "search _");
```

Dual-licensed MIT / Apache-2.0.
**[github.com/wongjiahau/anyfix-expr](https://github.com/wongjiahau/anyfix-expr)**

---

## Short-form copy

**crates.io / GitHub description (≤ 160 chars)**
> S-expressions without the parentheses: a lossless, zero-dependency parser
> for a readable notation that groups calls and leaves meaning to you.

**Social post / Hacker News title**
> Show HN: Anyfix expressions – S-expressions without the parentheses

**Tweet/Bluesky-length**
> Lisp separated reading from meaning. anyfix-expr does too, minus the parens.
> `IF x > 0 THEN x ELSE 0 - x` → call `IF _ THEN _ ELSE _`.
> 11 special chars, 2 operators, no precedence table, every JSON file parses,
> lossless round-trip. Rust, zero deps.

**Reddit (r/rust, r/ProgrammingLanguages) opener**
> I wanted to build a language without spending months on its parser, so I
> made the parser the part nobody has to write. anyfix-expr reads text into a
> tree of calls whose names wrap their arguments (`list file in _`,
> `_ <= _ < _`, `IF _ THEN _ ELSE _`), and assigns meaning to nothing.
> Feedback on the grouping rules is very welcome.

---

## Before publishing

- [ ] **Install snippet:** switch to `anyfix-expr = "0.1"` once the crate is
      on crates.io (the draft uses the git URL to be safe).
- [ ] **resh link:** confirm the repo is public, or drop that section.
- [ ] **Version:** the crate is 0.1.0; say whether the notation is stable or
      still open to change.
- [ ] Consider adding a side-by-side "S-expression vs anyfix" image or GIF
      for social posts.
