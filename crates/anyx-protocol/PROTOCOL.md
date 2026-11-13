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

ERRORS file errors [
    # The owner moved this file to the trash.
    trashed,
    missing,
],

# Saves the score, if the file is still at `revision`.
METHOD write (path: TEXT) at revision (revision: INTEGER) content (content: TEXT)
    RETURNS (new revision: INTEGER)
    ERRORS [
        # Someone saved first. Apply your change to `content`, then write at `revision`.
        conflict at revision (revision: INTEGER) content (content: TEXT),
    ] + file errors,
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
- **`METHOD shape RETURNS (name: TYPE) ERRORS errors`**: the method's
  shape, what it returns, and the errors it may return. The returned value
  has a name, which says what it is. `ERRORS` may be left out.
- **Errors** are written where they are used, in full: a shape like a
  variant's, with its `#` doc before it. A method's `ERRORS` has its own
  errors in brackets, then the groups it shares joined with `+`:
  `[conflict at revision (revision: INTEGER)] + file errors`, or a group
  alone, `file errors`.
- **`ERRORS name [errors]`** defines a group of errors that methods share.
  It may join other groups with `+`: `ERRORS export errors [unreadable] +
  file errors`. A group exists only to be shared; an error used by one
  method goes in that method's brackets.
- Each error is defined once in a scope, in the brackets of the one method
  or group that has it. To share an error, put it in a group.
- A parameter can't be named `kind`, which JSON uses for a variant's name.

Schemas are formatted with `anyx_protocol::format_schema`, which puts a long
`METHOD`'s `RETURNS` and `ERRORS` on lines of their own.

### Scopes

A schema can be a folder instead of one file. `protocol.anyx` at its root
holds only the `PROTOCOL` header, and every other `.anyx` file is a *scope*,
named by its path without `.anyx`:

```text
notes/
  protocol.anyx        PROTOCOL notes VERSION 1,
  common.anyx          scope `common`
  files.anyx           scope `files`
  files/export.anyx    scope `files/export`
```

A file or folder name is one lowercase word, letters and digits.

- **A method is called by its full name**: the scope's path, `/`, then its
  shape. `METHOD pdf (path: TEXT)` in `files/export.anyx` is called as
  `files/export/pdf "a.note"`. A method's own shape has no `/`.
- **Types and error groups are private** to the scope that declares them;
  folders give no special access, so `files/export` can't see what `files`
  keeps to itself. **`PUBLIC`** before `RECORD`, `CHOICE` or `ERRORS` shares
  it, and another scope then names it by its full path:

```anyx
## common.anyx
PUBLIC RECORD diagnostic { line: INTEGER, message: TEXT },
PUBLIC ERRORS common errors [missing],

## files.anyx
PUBLIC ERRORS file errors [trashed] + common/common errors,
METHOD read (path: TEXT) RETURNS (file: file) ERRORS file errors,

## files/export.anyx
METHOD pdf (path: TEXT) RETURNS (pdf: BYTES)
    ERRORS [unreadable (diagnostics: LIST common/diagnostic)] + files/file errors,
```

- In its own scope, a type or group is named without its path.
- An error is written on the wire by its words alone, `ERROR missing`, so a
  method can't list two errors of the same words from different scopes.

## Requests

A request is calls separated by commas, each in a method's shape with a value
in each slot:

```anyx
list files,
write "Hymns/a.poentaro" at revision 3 content """
    title "Ode"
    """,
```

In a schema of scopes, each call starts with its scope's path:
`files/export/pdf "a.note", files/list`. Spaces around the `/` are allowed.

Every call runs, in order.

`SCHEMA` is a built-in call every server answers, with no code of its own:
it returns the guide (below) as `TEXT`, without how to send a request,
since the caller just sent one. A client that has only a server's address
learns everything else from it:

```anyx
SCHEMA,
```

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
reads before its first request, and what `SCHEMA` answers (with
`how_to_send` empty): these rules, briefly, then the schema with
its `#` lines, a section per scope's file.
