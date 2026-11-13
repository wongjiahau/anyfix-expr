# The anyx protocol

An app declares its methods in a schema written in anyx. A request is an
anyx document of calls in the methods' shapes; the response is an anyx
document of their results. Both are checked against the schema.

## The schema

```anyx
# What the protocol is for, for whoever reads it first.
PROTOCOL poentaro VERSION 1,

# A file in the account's tree.
RECORD file summary {
    # A slash path, such as `Hymns/hymn.poentaro`.
    path: TEXT,
    revision: INTEGER,
    note: OPTIONAL: TEXT,
},

CHOICE shade { dark, light },

CHOICE colour { dark, light, custom hue (hue: NUMBER), (base: shade) dimmed },

# Someone saved first. Apply your change to `content`, then write at `revision`.
ERROR conflict at revision (revision: INTEGER) content (content: TEXT),
ERROR trashed,
ERROR missing,
ERRORS file errors [trashed, missing],

# Saves the score, if the file is still at `revision`.
METHOD write (path: TEXT) at revision (revision: INTEGER) content (content: TEXT)
    RETURNS (new revision: INTEGER)
    ERRORS [conflict at revision content, file errors],
```

- **`PROTOCOL name VERSION n`** comes first. `n` is a whole number from 1.
- **`#` lines** before a definition, a field or a variant document it. They
  are part of the schema, so the guide an agent reads includes them. `##`
  starts a comment.
- **Names** are lowercase words: `file summary`. All-caps words belong to the
  schema language. Keywords side by side join into one name, so write
  `LIST: TEXT` and `OPTIONAL: TEXT` with the `:`.
- **Types:** `TEXT`, `BYTES` (such as a file), `INTEGER` (64-bit), `NUMBER` (a finite float),
  `BOOLEAN`, `LIST: T`, and the name of a `RECORD` or `CHOICE`. A type may
  hold itself only through a `LIST`.
- **`RECORD name { field: TYPE, … }`**: fields by name. An `OPTIONAL` field
  may be left out.
- **`CHOICE name { variant, … }`**: one of its variants. A variant is a
  *shape*: words, with `(name: TYPE)` slots anywhere among them, as in
  `custom hue (hue: NUMBER)` or `(base: shade) dimmed`. Variants differ by
  their words.
- **`ERROR shape`** declares an error, a shape like a variant's. **`ERRORS
  name [a, b]`** names a list of errors, which can include other lists. A
  list refers to an error by its words alone: `conflict at revision content`.
- **`METHOD shape RETURNS (name: TYPE) ERRORS list`**: the method's shape,
  what it returns, and the errors it may return, as one name or a list in
  brackets. The returned value has a name, which says what it is. `ERRORS`
  may be left out.
- A parameter can't be named `kind`, which JSON uses for a variant's name.

Schemas are formatted with `anyx_protocol::format_schema`, which puts a long
`METHOD`'s `RETURNS` and `ERRORS` on lines of their own.

## Requests

A request is calls separated by commas, each in a method's shape with a value
in each slot:

```anyx
list files,
write "Hymns/a.poentaro" at revision 3 content """
    title "Ode"
    """,
```

Every call runs, in order.

## Values

| Type | Written |
| --- | --- |
| `TEXT` | `"text"` with `\"`, `\\`, `\n`, `\r`, `\t` and `\u{…}` escapes; or a raw string of three or more quotes, whose closing quotes' indentation is removed from every line |
| `BYTES` | its standard base64 (RFC 4648, padded) as a string: `"TVRoZA=="` |
| `INTEGER`, `NUMBER` | `3`, `-1.5` |
| `BOOLEAN` | `true`, `false` |
| `LIST: T` | `[a, b]` |
| a `RECORD` | `{ path "a", revision 3 }`, fields in any order; `name: value` when the value is a word |
| a `CHOICE` | the variant's shape filled in: `dark`, `custom hue 0.5` |

A value written as words, a `BOOLEAN` or a variant, goes in parentheses in a
slot, where it would otherwise join the words around it:
`set (custom hue 0.5) strict (true)`.

## Responses

One result per call, in order, separated by commas:

- the value the method returns;
- `ERROR` and one of the method's errors, filled in:
  `ERROR conflict at revision 4 content "…"`. A method that returns an error
  has changed nothing;
- `INTERNAL ERROR`: the server failed, and logged why. The calls after it
  don't run;
- `NOT RUN`: an earlier call failed with `INTERNAL ERROR`.

A request that can't be read gets one result in place of all of them, and
none of its calls run:

```anyx
INVALID REQUEST { line 1, column 6, message "found the call `reed _`; expected one of the methods …" }
```

Responses are laid out by `anyx::format`.

## JSON

With the `json` feature, values convert to and from JSON for a TypeScript
client calling the codec through wasm. Names are camelCase. A record is an
object; a variant or an error is an object whose `kind` is its words in
camelCase, next to its parameters:
`{ "kind": "conflictAtRevisionContent", "revision": 4, "content": "…" }`.
A result is `{ "kind": "value", "value": … }`, `{ "kind": "error",
"error": … }`, `{ "kind": "notRun" }`, `{ "kind": "internalError" }` or
`{ "kind": "invalidRequest", "line": …, "column": …, "message": … }`.

## The guide

`anyx_protocol::guide(schema, how_to_send)` writes the markdown an agent
reads before its first request: these rules, briefly, then the schema with
its `#` lines.
