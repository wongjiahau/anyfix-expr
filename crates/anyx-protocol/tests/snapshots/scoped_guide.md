# notes (anyx protocol, version 1)

Send a request as the body of `POST /notes`.

## Requests

A request is a list of calls separated by commas. Each call is written in the
shape of one of the methods below, with a value in each `(name: TYPE)` slot:

    files/write "a" at revision 3 content "a"

A method in a scope is called by its full name, as above: the scope's
path, `/`, then the call.

Every call runs, in order, and the response has one result per call.

`SCHEMA` is a call every server answers, with this guide.

## Values

- TEXT is a string: `"some text"`. Escape `\"` and `\\`, and write a line
  break as `\n`. For text with many lines, use a raw string: three quotes,
  the lines, then three quotes on a line of their own. The closing quotes'
  indentation is removed from every line. A raw string has no escapes.

      """
          first line
          second line
          """

- BYTES is base64 in a string: `"TVRoZA=="`.
- INTEGER is a whole number, NUMBER any number: `3`, `-1.5`.
- BOOLEAN is `true` or `false`.
- `LIST: T` is `[a, b, c]`.
- A RECORD is its fields in braces, each its name and value, in any order:
  `{ path "a", revision 3 }`. An `OPTIONAL` field may be left out. When a
  value is a word, put `:` after the field's name: `{ status: done }`.
- A CHOICE is one of its variants, written in the variant's shape with a
  value in each slot: `dark`, `custom hue 0.5`.
- In a slot, a value written as words goes in parentheses so that it doesn't
  join the words around it: `set (true)`, `pick (custom hue 0.5)`.

## Responses

The response has one result per call, in order. A result is one of:

- The value the method `RETURNS`.
- `ERROR` and one of the method's errors, in the error's shape. They follow
  `ERRORS`: its own in brackets, then those of each group joined with `+`.
  For example: `ERROR missing`, `ERROR conflict at revision 4 content "…"`.
  The call didn't do anything. Each error's `#` lines say what to do next.
- `INTERNAL ERROR`: the server failed. Calls after it don't run.
- `NOT RUN`: the call didn't run, because an earlier call failed that way.

A request that can't be read gets one result, and none of its calls run:

    INVALID REQUEST { line 1, column 6, message "…" }

## The schema

In the schema, a `#` line before a definition says what it means. Lowercase
words are names; all-caps words belong to the schema language. `ERRORS name
[…]` is a group of errors that methods share by joining it with `+`.

The schema is a file per scope, each shown below under its full name. A
type or an error group is private to its scope unless it is `PUBLIC`; another
scope writes its full path, such as `common/diagnostic`. A response writes
an error in its shape alone, without its scope: `ERROR missing`.

### `protocol.anyx`

# Notes keeps a person's notes, a file each.
PROTOCOL notes VERSION 1,

### `common` (`common.anyx`)

# One problem in a note.
PUBLIC RECORD diagnostic { line: INTEGER, message: TEXT },

PUBLIC ERRORS common errors [
    # No note has this path.
    missing,
],

### `files` (`files.anyx`)

Methods: `files/list`, `files/read _`, `files/write _ at revision _ content _`.

RECORD file { path: TEXT, revision: INTEGER, content: TEXT },

PUBLIC ERRORS file errors [
    # The note is in the trash.
    trashed,
] + common/common errors,

METHOD list RETURNS (paths: LIST: TEXT),

METHOD read (path: TEXT) RETURNS (file: file) ERRORS file errors,

# Saves the note, if it is still at `revision`.
METHOD write (path: TEXT) at revision (revision: INTEGER) content (content: TEXT)
    RETURNS (warnings: LIST common/diagnostic)
    ERRORS [
        # The note has errors, so nothing was saved.
        rejected (diagnostics: LIST common/diagnostic),
    ] + file errors,

### `files/export` (`files/export.anyx`)

Methods: `files/export/html _`, `files/export/pdf _`.

# A page of a note, with what is wrong on it.
RECORD page { html: TEXT, problem: OPTIONAL common/diagnostic },

METHOD html (path: TEXT) RETURNS (pages: LIST page) ERRORS files/file errors,

METHOD pdf (path: TEXT) RETURNS (pdf: BYTES)
    ERRORS [
        # The note has errors, so it can't be exported.
        unreadable (diagnostics: LIST common/diagnostic),
    ] + files/file errors,
