# anyfix: write a language, skip the parser

```text
WHEN error rate > 5 percent FOR 10 minute PAGE (on call)
```

That isn't pseudocode. **anyfix-expr** parses it into one call:

```text
WHEN _ FOR _ PAGE _      ← the name: its words, with _ for each hole
  error rate > 5 percent
  10 minute
  on call
```

Your program never sees text, tokens or precedence. It sees a call named
`WHEN _ FOR _ PAGE _` with three arguments, and decides what that means.

---

## The whole idea in four lines

| You write | Your code receives |
| --- | --- |
| `spawn 3 archer` | `spawn _ archer` (3) |
| `say "Who goes there?" as (guard)` | `say _ as _` ("Who goes there?", guard) |
| `cart total >= 50` | `_ >= _` (cart total, 50) |
| `IF ready THEN launch ELSE wait` | `IF _ THEN _ ELSE _` (ready, launch, wait) |

**The words form the name. Everything else fills the holes.** That's the
"any" in anyfix: a name can wrap its arguments from the front, the back, the
middle, or all three.

## Your interpreter is a `match`

The tree arrives fully grouped, so a language is a lookup table:

```rust
match call.name_text().as_str() {
    "_ + _"              => a[0] + a[1],
    "_ percent of _"     => a[0] / 100.0 * a[1],
    "average of _ and _" => (a[0] + a[1]) / 2.0,
    "IF _ THEN _ ELSE _" => if a[0] != 0.0 { a[1] } else { a[2] },
    other => panic!("unknown call `{other}`"),
}
```

```text
IF 15 percent of 80 > 10 THEN 1 ELSE 0   →  1
```

No tokenizer, no grammar file, no precedence climbing. A new feature is a new
line in the `match`.

## One parser, every little language

All of these parse with the same crate and no configuration:

```text
# pricing rules
rule (free shipping) : WHEN cart total >= 50 THEN shipping cost 0,

# a game script
ON player enter (castle gate) DO { say "Who goes there?" as (guard) },

# a query, as a pipeline of stages
FROM orders . WHERE status == "paid" . GROUP BY (customer) . LIMIT 10,

# a test
cart . items . count . SHOULD BE 3,

# a contract
REQUIRES amount <= balance,

# a step in a proof by induction
EQUALS 1 + length (rest ++ ys) BY (definition of length),
```

Each line becomes calls like `_ WHERE _`, `_ SHOULD BE _` and
`EQUALS _ BY _`. None of those words mean anything to anyfix. They mean
whatever your language says they mean.

## How it groups: three rules, two operators

1. **ALL-CAPS words split first**, so `IF … THEN … ELSE …` never needs
   parentheses.
2. **Symbols split next**, so `cart total >= 50` is `>=` between
   `cart total` and `50`.
3. **The remaining words name the call.**

Then `.` pipes everything on its left into the next phrase
(`"photo.jpg" . resize to 800 by 600 . save as "thumb.jpg"`), and `:` passes
the rest of the line in (`every day at 2am : back up "/var/db"`).

That's the whole notation. There's no precedence table to memorize, because
there's no precedence.

## Ready for real tools

- **Every JSON file is already valid anyfix.** Start from the config you
  have.
- **Lossless.** Printing the tree gives back the input byte for byte,
  comments included, even when it has errors.
- **A canonical formatter.** `format(source)` lays out any document one
  consistent way and never changes its meaning.
- **Errors that say how to fix them.** Every error has a stable code, an
  exact location and a one-line fix, readable by people and by AI agents.
- **Tiny.** 11 special characters, 2 operators, 0 dependencies.

## Try it

```toml
[dependencies]
anyfix-expr = { git = "https://github.com/wongjiahau/anyfix-expr" }
```

```rust
let parsed = anyfix_expr::parse("spawn 3 archer");
let nodes = parsed.syntax.nodes();
let anyfix_expr::NodeKind::Call(call) = &nodes[0].kind else { unreachable!() };
assert_eq!(call.name_text(), "spawn _ archer");
```

Rust · MIT or Apache-2.0 ·
[github.com/wongjiahau/anyfix-expr](https://github.com/wongjiahau/anyfix-expr)

---

## Short copy

**Repo description**
> Write a language, skip the parser. Reads plain, English-like text into a
> lossless tree of calls. Zero dependencies.

**Hacker News**
> Show HN: Anyfix – write a language without writing a parser

**Post**
> `WHEN error rate > 5 percent FOR 10 minute PAGE (on call)`
>
> Not pseudocode. It parses into one call, `WHEN _ FOR _ PAGE _`, with three
> arguments. Same parser does pricing rules, game scripts, query pipelines,
> tests and proofs. Your interpreter is a `match`. Rust, zero deps.

---

## Before publishing

- [ ] Switch the install line to `anyfix-expr = "0.1"` once it's on crates.io.
- [ ] Add the calculator as `examples/calc.rs` so readers can run it.
- [ ] Consider a GIF: type a line, watch its call name and holes appear.
