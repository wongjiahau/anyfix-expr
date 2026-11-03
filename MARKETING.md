# anyfix-expr — marketing draft

> Draft copy for a launch post, crates.io/GitHub blurb, and social posts.
> Every example below was run through the parser, and every tree shown is
> what `anyfix-expr` actually produces. Check the "Before publishing" list at
> the bottom first.

---

## Taglines (pick one)

- **One parser. Every little language.**
- **Write a language. Skip the parser.**
- **S-expressions without the parentheses.**
- **Syntax that reads like English and parses like Lisp.**

## One-liner

anyfix-expr is a zero-dependency Rust crate that reads plain, English-like
text into a tree of calls. Rule engines, game scripts, alerting configs and
query languages can all share it, so none of them needs its own parser.

---

## Launch post

### Five languages, zero parsers

Each block below is a different little language. None of them has a grammar
file, a lexer or a precedence table. The same crate parses all of them, with
no configuration.

**A pricing rules engine**

```text
rule (free shipping)  : WHEN cart total >= 50 THEN shipping cost 0,
rule (bulk discount)  : WHEN quantity >= 10 THEN discount 15 percent,
rule (loyal customer) : WHEN years active > 3 THEN add 500 point,
```

**A game script**

```text
ON player enter (castle gate) DO {
  play sound "creak.wav",
  say "Who goes there?" as (guard),
  IF player has (royal seal) THEN open (gate) ELSE spawn 3 archer,
}
```

**An alerting config**

```text
WHEN error rate > 5 percent FOR 10 minute PAGE (on call)
```

**A query language**

```text
SELECT [name, email] FROM users WHERE age >= 18 ORDER BY signup date
```

**An image pipeline**

```text
"photo.jpg" . resize to 800 by 600 . sharpen . save as "thumb.jpg"
```

Here's what your code receives for each. Every line becomes a **call whose
name is its words, with `_` where the arguments go**:

| Source | Call name | Arguments |
| --- | --- | --- |
| `WHEN cart total >= 50 THEN shipping cost 0` | `WHEN _ THEN _` | `cart total >= 50`, `shipping cost 0` |
| `say "Who goes there?" as (guard)` | `say _ as _` | `"Who goes there?"`, `guard` |
| `spawn 3 archer` | `spawn _ archer` | `3` |
| `WHEN error rate > 5 percent FOR 10 minute PAGE (on call)` | `WHEN _ FOR _ PAGE _` | `error rate > 5 percent`, `10 minute`, `on call` |
| `SELECT … FROM … WHERE … ORDER BY …` | `SELECT _ FROM _ WHERE _ ORDER BY _` | `[name, email]`, `users`, `age >= 18`, `signup date` |
| `… . resize to 800 by 600 . sharpen . save as "thumb.jpg"` | `_ save as _` | `(_ sharpen (_ resize to _ by _ …))`, `"thumb.jpg"` |

Arguments are calls too, all the way down: `error rate > 5 percent` arrives
as `_ > _` of `error rate` and `_ percent` of `5`.

### Your interpreter is a lookup table

Because the tree arrives fully grouped, evaluating a language is a `match` on
call names. This is a complete calculator with English operators:

```rust
use anyfix_expr::{parse, Node, NodeKind};

fn eval(node: &Node) -> f64 {
    let NodeKind::Call(call) = &node.kind else {
        if let NodeKind::Number(n) = &node.kind { return n.parse().unwrap() }
        panic!("unexpected node")
    };
    let a: Vec<f64> = call.arguments.iter().map(eval).collect();
    match call.name_text().as_str() {
        "_ + _" => a[0] + a[1],
        "_ * _" => a[0] * a[1],
        "_ > _" => (a[0] > a[1]) as u8 as f64,
        "_ percent of _" => a[0] / 100.0 * a[1],
        "average of _ and _" => (a[0] + a[1]) / 2.0,
        "IF _ THEN _ ELSE _" => if a[0] != 0.0 { a[1] } else { a[2] },
        other => panic!("unknown call `{other}`"),
    }
}
```

```text
15 percent of 80                        = 12
average of 3 and 7 .* 10                = 50
IF 15 percent of 80 > 10 THEN 1 ELSE 0  = 1
2 .+ 3 .* 4                             = 20
```

No tokenizer, no grammar, no precedence climbing. Adding a feature to the
language means adding a line to the `match`.

### The same rule, four ways

The free-shipping rule from above, written in formats people use for this
today:

**JSON**
```json
{ "rule": "free shipping",
  "when": { "op": ">=", "left": "cart.total", "right": 50 },
  "then": { "set": "shipping_cost", "value": 0 } }
```

**YAML**
```yaml
- rule: free shipping
  when: { op: ">=", left: cart.total, right: 50 }
  then: { set: shipping_cost, value: 0 }
```

**S-expression**
```lisp
(rule "free shipping" (when (>= cart-total 50)) (then (set shipping-cost 0)))
```

**Anyfix**
```text
rule (free shipping) : WHEN cart total >= 50 THEN shipping cost 0
```

The anyfix version is the one a product manager can read, review and edit,
and it is still a precise tree your program can check.

### How it groups, in three rules

1. **ALL-CAPS words split first.** `WHEN … FOR … PAGE …` needs no
   parentheses, however long the parts are.
2. **Symbols split next.** `cart total >= 50` is `>=` with `cart total` on
   one side and `50` on the other.
3. **The remaining words name the call; everything else fills its slots.**
   `say "Who goes there?" as (guard)` is `say _ as _`.

Then two operators chain calls together: `.` passes a value forward,
`:` passes the rest of the line in. That's the whole notation. There is no
precedence table to memorize: `2 .+ 3 .* 4` is 20, left to right, because it
says so.

- **11 special characters**: `, : . ( ) [ ] { } # "`. Nothing else is magic.
- **2 operators**: `.` and `:`. Both are sugar, rewritten away like Lisp's `'`.
- **1 separator**: `,`. Newlines are just whitespace.
- **0 dependencies.**

### Your JSON files already parse

Every JSON document is a valid anyfix document, escapes, exponents and
surrogate pairs included. Start from the config you have and let it grow
comments, trailing commas and readable rules where JSON used to need nested
objects.

### Built for tooling, not just parsing

- **Lossless.** `to_source()` gives back the input byte for byte, valid or
  not, comments included. Build formatters, linters and language servers on
  the same tree.
- **Never gives up.** The parser recovers and reports every error, sorted by
  position, and caps nesting so hostile input can't overflow the stack.
- **Errors that tell you the fix.** Each diagnostic has a stable code, a span
  and a one-line message saying what was found, what was expected and how to
  fix it. That works for people and for AI agents repairing their own output.
- **Embed other languages.** Raw strings (`"""`) carry SQL, regexes or shader
  code as written, and your embedded parser's errors can still point to the
  exact spot in the file.

### Who it's for

- **Product teams** who want business rules that non-engineers can read.
- **Game and app developers** who want a scripting or config language without
  shipping a parser.
- **Language designers** who want to prototype semantics, not syntax.
- **AI and agent builders**, where a small, regular syntax and fix-it error
  messages make generated code easy to check and repair.

### Already powering a shell

anyfix-expr is the reader behind
[resh](https://github.com/wongjiahau/resh), a readable, typed shell, and the
crate knows nothing about it.

### Try it

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

Dual-licensed MIT / Apache-2.0.
**[github.com/wongjiahau/anyfix-expr](https://github.com/wongjiahau/anyfix-expr)**

---

## Short-form copy

**crates.io / GitHub description (≤ 160 chars)**
> One parser for every little language: reads English-like text into a
> lossless tree of calls. Rules, scripts, configs. Zero dependencies.

**Hacker News title**
> Show HN: Anyfix – one parser for rule engines, game scripts and configs

**Tweet/Bluesky-length**
> `WHEN error rate > 5 percent FOR 10 minute PAGE (on call)`
> That's not pseudocode. It parses into the call `WHEN _ FOR _ PAGE _`
> with three arguments. Same parser handles game scripts, pricing rules and
> SQL-ish queries. Your interpreter is a `match`. Rust, zero deps.

**Second tweet (thread)**
> A whole calculator with English operators:
> `"_ percent of _" => a[0] / 100.0 * a[1],`
> `"average of _ and _" => (a[0] + a[1]) / 2.0,`
> No tokenizer. No grammar. No precedence table.

**Reddit (r/rust, r/ProgrammingLanguages) opener**
> I kept writing the same parser for every small language: a rule engine, a
> config format, a scripting layer. So I wrote one notation they can all
> share. `ON player enter (castle gate) DO { … }`, `SELECT … FROM … WHERE …`
> and `WHEN cart total >= 50 THEN shipping cost 0` all parse with the same
> crate, into calls like `ON _ DO _` that you evaluate with a `match`.
> I'd love feedback on the grouping rules.

---

## Before publishing

- [ ] **Install snippet:** switch to `anyfix-expr = "0.1"` once the crate is
      on crates.io (the draft uses the git URL to be safe).
- [ ] **resh link:** confirm the repo is public, or drop that section.
- [ ] **Version:** the crate is 0.1.0; say whether the notation is stable or
      still open to change.
- [ ] **Calculator example:** consider adding it to the repo as
      `examples/calc.rs` so readers can `cargo run --example calc`.
- [ ] Consider a GIF that types one of the gallery lines and shows its tree.
