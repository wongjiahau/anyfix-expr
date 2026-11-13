# poentaro (anyx protocol, version 1)

Send a request as the body of `POST https://example.com/agent`.

## Requests

A request is a list of calls separated by commas. Each call is written in the
shape of one of the methods below, with a value in each `(name: TYPE)` slot:

    write "a" at revision 3 content "a"

Every call runs, in order, and the response has one result per call.

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
- `ERROR` and one of the errors the method lists after `ERRORS`, in the
  error's shape: `ERROR missing`, `ERROR conflict at revision 4 content "…"`.
  The call didn't do anything. Each error's `#` lines say what to do next.
- `INTERNAL ERROR`: the server failed. Calls after it don't run.
- `NOT RUN`: the call didn't run, because an earlier call failed that way.

A request that can't be read gets one result, and none of its calls run:

    INVALID REQUEST { line 1, column 6, message "…" }

## The schema

In the schema, a `#` line before a definition says what it means. Lowercase
words are names; all-caps words belong to the schema language. `ERRORS name
[…]` names a list of errors that methods share.

# Poentaro turns `.poentaro` text into numbered musical notation (jianpu) and
# keeps the account owner's scores in the cloud. When you save, anyone who
# has the score open sees the change.
#
# Read https://poentaro.com/syntax.md before you write a score.
#
# To edit a score: read it, change the text, then write it at the revision
# you read. If the write comes back as a conflict, apply your change again
# to the content it returns and write at the revision it returns.
PROTOCOL poentaro VERSION 1,

# A file in the account's tree.
RECORD file summary {
    # A slash path, such as `Hymns/hymn.poentaro`.
    path: TEXT,
    revision: INTEGER,
},

RECORD file { path: TEXT, revision: INTEGER, content: TEXT },

# One problem in a score.
RECORD diagnostic {
    # Counted from 1, in the content you sent.
    line: INTEGER,
    # Counted from 1, in characters.
    column: INTEGER,
    code: TEXT,
    message: TEXT,
},

# The owner moved this file to the trash. Ask them to restore it.
ERROR trashed,

# No file has this path. It may have been renamed: run `list files` to see
# the exact paths.
ERROR missing,

# Someone saved first. Apply your change to `content`, then write at
# `revision`.
ERROR conflict at revision (revision: INTEGER) content (content: TEXT),

# The score doesn't parse. Nothing was saved.
ERROR rejected (diagnostics: LIST diagnostic),

# A file already has this path. Read it, or create under another path.
ERROR already exists at revision (revision: INTEGER),

# The path isn't valid, such as an empty folder name or a `..`.
ERROR invalid path (message: TEXT),

ERRORS file errors [trashed, missing],

# Every file this account owns.
METHOD list files RETURNS files: LIST file summary,

METHOD read (path: TEXT) RETURNS (file: file) ERRORS file errors,

# Saves the score, if the file is still at `revision`.
METHOD write (path: TEXT) at revision (revision: INTEGER) content (content: TEXT)
    RETURNS (new revision: INTEGER)
    ERRORS [conflict at revision content, rejected, file errors],

# Makes a new file, and its folders if they don't exist.
METHOD create (path: TEXT) content (content: TEXT)
    RETURNS (revision: INTEGER)
    ERRORS [already exists at revision, invalid path, rejected],

# The score as a Standard MIDI File.
METHOD export midi (path: TEXT)
    RETURNS (midi: BYTES)
    ERRORS file errors,
