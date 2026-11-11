use crate::error::{Error, ErrorCode};
use crate::span::Span;
use crate::string::StringLiteral;
use crate::syntax::{Token, TokenKind, Trivia, TriviaKind};

/// The eleven characters that never appear inside a word.
pub(crate) const SPECIAL: [char; 11] = [',', ':', '.', '(', ')', '[', ']', '{', '}', '#', '"'];

fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `alnum` in the grammar: a letter, an ASCII digit or `_`.
fn is_alnum(c: char) -> bool {
    c.is_alphabetic() || c.is_ascii_digit() || c == '_'
}

/// Whether a word token is made of symbol characters (`<=`, `+`), not a name.
/// A word never mixes the two, so its first character decides.
pub fn is_symbol_word(text: &str) -> bool {
    text.chars().next().is_some_and(is_symbol)
}

/// A name word in all caps: at least one uppercase letter and no lowercase
/// one, so `IF`, `T` and `X1` are keywords but `If`, `_1` and `中` are not.
pub fn is_keyword(text: &str) -> bool {
    !is_symbol_word(text)
        && text.chars().any(char::is_uppercase)
        && !text.chars().any(char::is_lowercase)
}

fn is_symbol(c: char) -> bool {
    !c.is_whitespace() && !SPECIAL.contains(&c) && !is_alnum(c)
}

/// All tokens of `source`, ending with [`TokenKind::End`], and the string errors.
pub(crate) fn lex(source: &str) -> (Vec<Token>, Vec<Error>) {
    let mut lexer = Lexer {
        source,
        position: 0,
        errors: Vec::new(),
    };
    let mut tokens = Vec::new();
    loop {
        let leading = lexer.trivia(false);
        let token = lexer.token(leading);
        let done = token.kind == TokenKind::End;
        tokens.push(token);
        if done {
            return (tokens, lexer.errors);
        }
    }
}

struct Lexer<'a> {
    source: &'a str,
    position: usize,
    errors: Vec<Error>,
}

impl Lexer<'_> {
    fn rest(&self) -> &str {
        &self.source[self.position..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_second(&self) -> Option<char> {
        self.rest().chars().nth(1)
    }

    fn bump(&mut self) {
        if let Some(c) = self.peek() {
            self.position += c.len_utf8();
        }
    }

    fn bump_while(&mut self, keep: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&keep) {
            self.bump();
        }
    }

    fn text(&self, start: usize) -> String {
        self.source[start..self.position].to_string()
    }

    /// Whitespace and comments. With `same_line`, stops before a newline.
    fn trivia(&mut self, same_line: bool) -> Vec<Trivia> {
        let mut trivia = Vec::new();
        loop {
            let start = self.position;
            let kind = match self.peek() {
                // A byte order mark that starts the input is whitespace.
                Some('\u{FEFF}') if self.position == 0 => {
                    self.bump();
                    TriviaKind::Whitespace
                }
                Some('#') => {
                    self.bump_while(|c| c != '\n');
                    TriviaKind::Comment
                }
                Some(c) if c.is_whitespace() && !(same_line && c == '\n') => {
                    self.bump_while(|c| c.is_whitespace() && !(same_line && c == '\n'));
                    TriviaKind::Whitespace
                }
                _ => return trivia,
            };
            trivia.push(Trivia {
                kind,
                text: self.text(start),
                span: Span::new(start, self.position),
            });
        }
    }

    fn token(&mut self, leading: Vec<Trivia>) -> Token {
        let start = self.position;
        let kind = match self.peek() {
            None => TokenKind::End,
            Some(c) => {
                self.bump();
                match c {
                    ',' => TokenKind::Comma,
                    ':' => TokenKind::Colon,
                    '.' => TokenKind::Dot,
                    '(' => TokenKind::OpenParen,
                    ')' => TokenKind::CloseParen,
                    '[' => TokenKind::OpenBracket,
                    ']' => TokenKind::CloseBracket,
                    '{' => TokenKind::OpenBrace,
                    '}' => TokenKind::CloseBrace,
                    '"' => self.string(start),
                    c if c.is_ascii_digit() || (c == '-' && self.at_digit()) => {
                        self.number(start);
                        TokenKind::Number
                    }
                    c if is_name_start(c) => {
                        self.bump_while(is_name_continue);
                        TokenKind::Word
                    }
                    _ => {
                        // A `-` before a digit starts a negative number, so it
                        // ends the symbol word: `<-5` is `<` then `-5`.
                        while self.peek().is_some_and(is_symbol)
                            && !(self.peek() == Some('-')
                                && self.peek_second().is_some_and(|c| c.is_ascii_digit()))
                        {
                            self.bump();
                        }
                        TokenKind::Word
                    }
                }
            }
        };
        let span = Span::new(start, self.position);
        let text = self.text(start);
        let trailing = self.trivia(true);
        Token {
            kind,
            text,
            span,
            leading,
            trailing,
        }
    }

    /// Whether an ASCII digit comes next.
    fn at_digit(&self) -> bool {
        self.peek().is_some_and(|c| c.is_ascii_digit())
    }

    /// The rest of a number after its first character (a digit, or the `-` of
    /// a negative number). A number is `0x`, `0b` or `0o` and digits in that
    /// base, or decimal digits with an optional fraction and exponent, as in
    /// `2.5e-3`. `_` separates digits. Any other letter ends the number, so
    /// `1mb` is `1` then `mb`, and every number has a value.
    fn number(&mut self, start: usize) {
        if self.source[start..self.position] == *"-" {
            self.bump();
        }
        let first = self.position - 1;
        let base_digit: Option<fn(char) -> bool> = match self.peek() {
            Some('x' | 'X') => Some(|c| c.is_ascii_hexdigit()),
            Some('b' | 'B') => Some(|c| matches!(c, '0' | '1')),
            Some('o' | 'O') => Some(|c| matches!(c, '0'..='7')),
            _ => None,
        };
        if let Some(is_digit) = base_digit
            && self.source[first..].starts_with('0')
            && self.peek_nth(1).is_some_and(is_digit)
        {
            self.bump();
            self.digits(is_digit);
            return;
        }
        let is_digit = |c: char| c.is_ascii_digit();
        self.digits(is_digit);
        if self.peek() == Some('.') && self.peek_nth(1).is_some_and(is_digit) {
            self.bump();
            self.digits(is_digit);
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            let sign = usize::from(matches!(self.peek_nth(1), Some('+' | '-')));
            if self.peek_nth(1 + sign).is_some_and(is_digit) {
                self.bump();
                if sign == 1 {
                    self.bump();
                }
                self.digits(is_digit);
            }
        }
    }

    /// Digits, with `_` between them: `1_000`. A `_` with no digit after it
    /// is not part of the number.
    fn digits(&mut self, is_digit: impl Fn(char) -> bool) {
        loop {
            match self.peek() {
                Some(c) if is_digit(c) => self.bump(),
                Some('_') if self.peek_nth(1).is_some_and(&is_digit) => self.bump(),
                _ => return,
            }
        }
    }

    /// The character `n` places after the next one.
    fn peek_nth(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    /// The rest of a string after its opening `"`. Three or more quotes open a
    /// raw string.
    fn string(&mut self, start: usize) -> TokenKind {
        if self.rest().starts_with("\"\"") {
            return TokenKind::String(Box::new(self.raw_string(start)));
        }
        let mut literal = StringLiteral::new(self.position);
        loop {
            match self.peek() {
                None => {
                    self.errors.push(Error::new(
                        ErrorCode::UnterminatedString,
                        Span::new(start, self.position),
                        "found a string that runs to the end of input; expected a closing `\"`; add `\"` where the string ends",
                    ));
                    break;
                }
                Some('"') => {
                    literal.content.end = self.position;
                    self.bump();
                    return TokenKind::String(Box::new(literal));
                }
                Some('\\') => {
                    let escape_start = self.position;
                    self.bump();
                    match self.escape(escape_start) {
                        Some(decoded) => {
                            literal.push_escape(decoded, Span::new(escape_start, self.position))
                        }
                        None => literal.push_verbatim(
                            &self.source[escape_start..self.position],
                            Span::new(escape_start, self.position),
                        ),
                    }
                }
                Some(_) => {
                    let char_start = self.position;
                    self.bump();
                    literal.push_verbatim(
                        &self.source[char_start..self.position],
                        Span::new(char_start, self.position),
                    );
                }
            }
        }
        literal.content.end = self.position;
        TokenKind::String(Box::new(literal))
    }

    /// The rest of an escape after its `\`, decoded. Escape letters are
    /// case-insensitive. An invalid escape is reported and returns `None`, so
    /// the caller keeps its source text in the value.
    fn escape(&mut self, start: usize) -> Option<char> {
        // At the end of input the string is unterminated; `string` reports it.
        let c = self.peek()?;
        self.bump();
        let decoded = match c.to_ascii_lowercase() {
            '"' => '"',
            '\\' => '\\',
            '/' => '/',
            'b' => '\u{8}',
            'f' => '\u{c}',
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'u' => return self.unicode_escape(start),
            _ => {
                let found = self.text(start);
                self.invalid_escape(
                    start,
                    format!(
                        "found the unknown escape `{found}` in a string; expected one of `\\\"` `\\\\` `\\/` `\\b` `\\f` `\\n` `\\t` `\\r` `\\u{{…}}` `\\uXXXX`; write `\\\\` for a literal backslash"
                    ),
                );
                return None;
            }
        };
        Some(decoded)
    }

    /// The rest of a `\u` escape after its `u`: `{…}` with 1 to 6 hex digits
    /// naming a Unicode scalar value, or JSON's 4 hex digits, where a UTF-16
    /// surrogate pair is two such escapes in a row.
    fn unicode_escape(&mut self, start: usize) -> Option<char> {
        if self.peek() == Some('{') {
            return self.braced_unicode_escape(start);
        }
        let Some(unit) = self.four_hex_digits() else {
            let found = self.text(start);
            self.invalid_escape(
                start,
                format!(
                    "found `{found}` in a string; expected `\\u{{` then 1 to 6 hex digits then `}}`, or `\\u` then exactly 4 hex digits; write the code point as `\\u{{41}}`"
                ),
            );
            return None;
        };
        if let Some(c) = char::from_u32(unit) {
            return Some(c);
        }
        if (0xD800..0xDC00).contains(&unit) {
            let rest = self.rest();
            let low = rest
                .strip_prefix("\\u")
                .or_else(|| rest.strip_prefix("\\U"))
                .and_then(|after| after.get(..4))
                .filter(|digits| digits.chars().all(|c| c.is_ascii_hexdigit()))
                .and_then(|digits| u32::from_str_radix(digits, 16).ok())
                .filter(|low| (0xDC00..0xE000).contains(low));
            if let Some(low) = low {
                self.position += 6;
                return char::from_u32(0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00));
            }
        }
        let found = self.text(start);
        self.invalid_escape(
            start,
            format!(
                "found `{found}` in a string, a UTF-16 surrogate without its pair; expected a high surrogate (D800 to DBFF) followed by a low one (DC00 to DFFF); write the code point as `\\u{{…}}` instead"
            ),
        );
        None
    }

    /// Exactly 4 hex digits, consumed, as a number.
    fn four_hex_digits(&mut self) -> Option<u32> {
        let digits = self.rest().get(..4)?;
        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let unit = u32::from_str_radix(digits, 16).ok()?;
        self.position += 4;
        Some(unit)
    }

    /// The rest of `\u{…}` after its `u`: 1 to 6 hex digits naming a Unicode scalar value.
    fn braced_unicode_escape(&mut self, start: usize) -> Option<char> {
        self.bump();
        let digits_start = self.position;
        self.bump_while(|c| c.is_ascii_hexdigit());
        let digits = self.text(digits_start);
        let closed = self.peek() == Some('}');
        if closed {
            self.bump();
        }
        let found = self.text(start);
        let scalar = u32::from_str_radix(&digits, 16)
            .ok()
            .and_then(char::from_u32);
        let problem = match (closed, digits.len(), scalar) {
            (false, _, _) => "has no closing `}`",
            (true, 0, _) => "has no hex digits",
            (true, 7.., _) => "has more than 6 hex digits",
            (true, _, None) => "is not a Unicode scalar value",
            (true, _, Some(c)) => return Some(c),
        };
        self.invalid_escape(
            start,
            format!(
                "found `{found}` in a string, which {problem}; expected `\\u{{` then 1 to 6 hex digits naming a Unicode scalar value (at most 10FFFF, not D800 to DFFF) then `}}`; fix the code point"
            ),
        );
        None
    }

    fn invalid_escape(&mut self, start: usize, message: String) {
        self.errors.push(Error::new(
            ErrorCode::InvalidEscape,
            Span::new(start, self.position),
            message,
        ));
    }

    /// The rest of a raw string after its first `"`: more quotes, the end of
    /// the line, then content lines up to a line holding only indentation and
    /// as many quotes as opened it. That indentation is stripped from every
    /// line, and the line breaks after the opening quotes and before the
    /// closing line are not part of the value. There are no escapes.
    fn raw_string(&mut self, start: usize) -> StringLiteral {
        self.bump_while(|c| c == '"');
        let quotes = self.text(start);
        self.bump_while(is_indentation);
        let extra_start = self.position;
        self.bump_while(|c| c != '\n');
        let extra = self.source[extra_start..self.position]
            .trim_end_matches(|c| c == '\r' || is_indentation(c));
        if !extra.is_empty() {
            self.errors.push(Error::new(
                ErrorCode::RawStringOpening,
                Span::new(extra_start, extra_start + extra.len()),
                format!(
                    "found `{extra}` after the opening `{quotes}` of a raw string; expected the end of the line; start the content on the next line, or use a `\"…\"` string for one line"
                ),
            ));
        }
        self.bump();
        let mut lines = Vec::new();
        loop {
            let line_start = self.position;
            self.bump_while(is_indentation);
            let indentation_end = self.position;
            let run = self.rest().chars().take_while(|&c| c == '"').count();
            if run == quotes.len() {
                self.position += run;
                return self.raw_value(&lines, Span::new(line_start, indentation_end));
            }
            self.bump_while(|c| c != '\n');
            lines.push(Span::new(line_start, self.position));
            if self.peek().is_none() {
                break;
            }
            self.bump();
        }
        self.errors.push(Error::new(
            ErrorCode::UnterminatedString,
            Span::new(start, self.position),
            format!(
                "found a raw string that runs to the end of input; expected a line holding only `{quotes}`; add `{quotes}` on its own line where the string ends"
            ),
        ));
        let content_start = lines.first().map_or(self.position, |line| line.start);
        let mut literal = StringLiteral::new(content_start);
        literal.push_verbatim(
            &self.source[content_start..self.position],
            Span::new(content_start, self.position),
        );
        literal.content.end = self.position;
        literal
    }

    /// The value of a raw string's content `lines`, each stripped of the
    /// closing line's `indentation`.
    fn raw_value(&mut self, lines: &[Span], indentation: Span) -> StringLiteral {
        let prefix = &self.source[indentation.start..indentation.end];
        let content_start = lines.first().map_or(indentation.start, |line| line.start);
        let mut literal = StringLiteral::new(content_start);
        literal.content.end = indentation.start;
        for (index, line) in lines.iter().enumerate() {
            let text = self.source[line.start..line.end].trim_end_matches('\r');
            let blank = text.chars().all(is_indentation);
            let start = if blank {
                line.start + text.len()
            } else if text.starts_with(prefix) {
                line.start + prefix.len()
            } else {
                let shared = text
                    .bytes()
                    .zip(prefix.bytes())
                    .take_while(|(a, b)| a == b)
                    .count();
                let own = text.chars().take_while(|&c| is_indentation(c)).count();
                self.errors.push(Error::new(
                    ErrorCode::RawStringIndentation,
                    Span::new(line.start, line.start + own),
                    format!(
                        "found a raw string line whose indentation ({own} spaces or tabs) does not start with the closing line's ({}); expected every non-blank line to start with the closing line's indentation; indent the line to match, or move the closing quotes left",
                        prefix.len()
                    ),
                ));
                line.start + shared
            };
            // The line break before the closing line is not part of the value.
            let end = match lines.get(index + 1) {
                Some(next) => next.start,
                None => line.start + text.len(),
            };
            literal.push_verbatim(&self.source[start..end], Span::new(start, end));
        }
        literal
    }
}

/// The whitespace a raw string's indentation is made of.
fn is_indentation(c: char) -> bool {
    c == ' ' || c == '\t'
}
