/**
 * Syntax highlighting for anyx documents in the Monaco editor.
 *
 * Copy this file into your app, then call `registerAnyx` once before creating
 * an editor:
 *
 *   import * as monaco from 'monaco-editor'
 *   import { registerAnyx } from './anyx-monaco'
 *
 *   registerAnyx(monaco, { languageId: 'anyx' })
 *   monaco.editor.create(element, { language: 'anyx', theme: 'anyx-light' })
 *
 * The highlighting is the same for every anyx document, whatever its grammar.
 * Seven classes of text get their own color:
 *
 *   keyword      a name word in all caps: `IF`, `ENTRY`, `X1`
 *   operator     a symbol word: `+`, `<=`, `->`
 *   identifier   any other name word: `port`, `If`, `_1`
 *   number       `42`, `-1.5`, `1_000`, `2.5e-3`, `0xFF`
 *   string       `"…"`, `"""` raw strings, and `#` line strings
 *   comment      `##` to the end of the line
 *   delimiter    `,` `:` `.` and the brackets `( ) [ ] { }`
 *
 * The rules mirror the lexer in crates/anyx/src/lexer.rs and the README's
 * "Tokenization rules". Copy the file again when those change.
 */

import type * as Monaco from 'monaco-editor'

type MonacoApi = typeof Monaco

/** Letters that may start a name (`char::is_alphabetic`), and `_`. */
const nameStart = String.raw`[\p{Alphabetic}_]`
/** Characters that may continue a name (`char::is_alphanumeric`), and `_`. */
const nameContinue = String.raw`[\p{Alphabetic}\p{N}_]`
/** A character of a symbol word: anything but whitespace, a special
 * character, a letter, an ASCII digit or `_`. */
const symbolCharacter = String.raw`[^\s\p{Alphabetic}0-9_,:.()\[\]{}#"]`

/** The tokenizer. Rules are tried in order, and the first match wins. */
export const anyxMonarchLanguage: Monaco.languages.IMonarchLanguage = {
  unicode: true,
  defaultToken: 'invalid',
  brackets: [
    { open: '(', close: ')', token: 'delimiter.parenthesis' },
    { open: '[', close: ']', token: 'delimiter.square' },
    { open: '{', close: '}', token: 'delimiter.curly' },
  ],
  tokenizer: {
    root: [
      [/\s+/, 'white'],
      // A byte order mark at the start of a document is whitespace.
      [/\uFEFF/, 'white'],
      // `##` starts a comment before anything else, even right after a word.
      [/##.*$/, 'comment'],
      // A single `#` starts a line string, which runs to the end of the line.
      [/#.*$/, 'string'],
      // Three or more quotes at the end of a line open a raw string, closed by
      // a line holding only indentation and as many quotes.
      [/("{3,})(?=[ \t]*$)/, { token: 'string.quote', next: '@raw.$1' }],
      [/"/, { token: 'string.quote', next: '@string' }],
      // A `-` before an ASCII digit always starts a number.
      [/-?0[xX][0-9a-fA-F](?:_?[0-9a-fA-F])*/, 'number'],
      [/-?0[bB][01](?:_?[01])*/, 'number'],
      [/-?0[oO][0-7](?:_?[0-7])*/, 'number'],
      [/-?[0-9](?:_?[0-9])*(?:\.[0-9](?:_?[0-9])*)?(?:[eE][+-]?[0-9](?:_?[0-9])*)?/, 'number'],
      // A name in all caps (an uppercase letter and no lowercase one) is a
      // keyword.
      [
        new RegExp(`${nameStart}${nameContinue}*`, 'u'),
        {
          // Monarch anchors a `~` guard to the whole word.
          cases: {
            '~[^\\p{Lowercase}]*\\p{Uppercase}[^\\p{Lowercase}]*': 'keyword',
            '@default': 'identifier',
          },
        },
      ],
      [/[()[\]{}]/, '@brackets'],
      [/[,:.]/, 'delimiter'],
      // A symbol word stops before a `-` that starts a number: `<-5` is `<`
      // then `-5`.
      [new RegExp(`(?:(?!-[0-9])${symbolCharacter})+`, 'u'), 'operator'],
    ],
    string: [
      [/[^\\"]+/, 'string'],
      // Escape letters are case-insensitive. Monarch ignores a regex's own
      // flags, so both cases are spelled out.
      [/\\(?:["\\/bBfFnNtTrR]|[uU]\{[0-9a-fA-F]{1,6}\}|[uU][0-9a-fA-F]{4})/, 'string.escape'],
      [/\\./, 'string.escape.invalid'],
      [/"/, { token: 'string.quote', next: '@pop' }],
    ],
    // `$S2` is the run of quotes that opened the raw string.
    raw: [
      [
        /^[ \t]*("{3,})/,
        {
          cases: {
            '$1==$S2': { token: 'string.quote', next: '@pop' },
            '@default': 'string',
          },
        },
      ],
      [/.+/, 'string'],
    ],
  },
}

/** Comments, brackets and auto-closing pairs. */
export const anyxLanguageConfiguration: Monaco.languages.LanguageConfiguration = {
  comments: { lineComment: '##' },
  brackets: [
    ['(', ')'],
    ['[', ']'],
    ['{', '}'],
  ],
  // Brackets take the delimiter color, not a color for each nesting depth.
  colorizedBracketPairs: [],
  autoClosingPairs: [
    { open: '(', close: ')' },
    { open: '[', close: ']' },
    { open: '{', close: '}' },
    { open: '"', close: '"', notIn: ['string', 'comment'] },
  ],
  surroundingPairs: [
    { open: '(', close: ')' },
    { open: '[', close: ']' },
    { open: '{', close: '}' },
    { open: '"', close: '"' },
  ],
}

/** Theme rules for a light background. Spread them into your own theme's
 * `rules` to keep your theme. */
export const anyxLightThemeRules: Monaco.editor.ITokenThemeRule[] = [
  { token: 'keyword', foreground: '0000ff', fontStyle: 'bold' },
  { token: 'operator', foreground: 'af00db' },
  { token: 'identifier', foreground: '001080' },
  { token: 'number', foreground: '098658' },
  { token: 'string', foreground: 'a31515' },
  { token: 'string.escape', foreground: 'ee0000' },
  { token: 'comment', foreground: '008000', fontStyle: 'italic' },
  { token: 'delimiter', foreground: '777777' },
  { token: 'invalid', foreground: 'cd3131' },
]

/** Theme rules for a dark background. */
export const anyxDarkThemeRules: Monaco.editor.ITokenThemeRule[] = [
  { token: 'keyword', foreground: '569cd6', fontStyle: 'bold' },
  { token: 'operator', foreground: 'c586c0' },
  { token: 'identifier', foreground: '9cdcfe' },
  { token: 'number', foreground: 'b5cea8' },
  { token: 'string', foreground: 'ce9178' },
  { token: 'string.escape', foreground: 'd7ba7d' },
  { token: 'comment', foreground: '6a9955', fontStyle: 'italic' },
  { token: 'delimiter', foreground: '808080' },
  { token: 'invalid', foreground: 'f44747' },
]

export interface RegisterAnyxOptions {
  /** The language id to register, such as `'anyx'` or your format's name. */
  languageId: string
  /** File extensions to associate with the language, such as `['.anyx']`. */
  extensions?: string[]
}

/**
 * Registers the language under `options.languageId`, and defines the themes
 * `anyx-light` and `anyx-dark`. Calling it again for the same id does nothing.
 */
export function registerAnyx(monaco: MonacoApi, options: RegisterAnyxOptions): void {
  const { languageId, extensions } = options
  if (monaco.languages.getLanguages().some((language) => language.id === languageId)) {
    return
  }
  monaco.languages.register({ id: languageId, extensions })
  monaco.languages.setLanguageConfiguration(languageId, anyxLanguageConfiguration)
  monaco.languages.setMonarchTokensProvider(languageId, anyxMonarchLanguage)
  monaco.editor.defineTheme('anyx-light', {
    base: 'vs',
    inherit: true,
    rules: anyxLightThemeRules,
    colors: {},
  })
  monaco.editor.defineTheme('anyx-dark', {
    base: 'vs-dark',
    inherit: true,
    rules: anyxDarkThemeRules,
    colors: {},
  })
}
