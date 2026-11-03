//! A string literal's decoded value, and where each part of it came from.

use crate::span::Span;

/// A string literal: its decoded value, and where each part of the value came
/// from in the source.
///
/// A consumer that parses a string's content (an embedded language in a raw
/// string, say) uses [`StringLiteral::source_offset`] to point its own errors
/// back into the file, past escapes and stripped indentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringLiteral {
    pub value: String,
    /// The parts of `value` in order, each with the source it came from.
    pub pieces: Vec<StringPiece>,
    /// The source between the delimiters. For a raw string, the lines between
    /// the opening and closing lines.
    pub content: Span,
}

/// A part of a string's value and the source it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringPiece {
    /// Where the piece starts in the value, in bytes.
    pub value_start: usize,
    /// Where the piece came from in the source.
    pub source: Span,
    /// The piece is the source copied byte for byte, so each of its value bytes
    /// maps to one source byte. An escape is not verbatim: its whole value maps
    /// to the start of the escape.
    pub verbatim: bool,
}

impl StringLiteral {
    /// The source offset of the value byte at `value_offset`. The value's end
    /// maps to the end of the last piece.
    ///
    /// ```
    /// let source = "x \"\"\"\n    ab\n    cd\n    \"\"\"";
    /// let nodes = anyfix_expr::parse(source).syntax.nodes();
    /// let anyfix_expr::NodeKind::Call(call) = &nodes[0].kind else { unreachable!() };
    /// let anyfix_expr::NodeKind::String(literal) = &call.arguments[0].kind else { unreachable!() };
    /// assert_eq!(literal.value, "ab\ncd");
    /// let offset = literal.source_offset(literal.value.find("cd").unwrap());
    /// assert_eq!(&source[offset..offset + 2], "cd");
    /// ```
    pub fn source_offset(&self, value_offset: usize) -> usize {
        let index = self
            .pieces
            .partition_point(|piece| piece.value_start <= value_offset);
        let Some(piece) = index.checked_sub(1).map(|index| &self.pieces[index]) else {
            return self.content.start;
        };
        let piece_end = self
            .pieces
            .get(index)
            .map_or(self.value.len(), |next| next.value_start);
        if value_offset >= piece_end {
            piece.source.end
        } else if piece.verbatim {
            piece.source.start + (value_offset - piece.value_start)
        } else {
            piece.source.start
        }
    }

    pub(crate) fn new(content_start: usize) -> Self {
        StringLiteral {
            value: String::new(),
            pieces: Vec::new(),
            content: Span::empty(content_start),
        }
    }

    /// Appends source text copied as written, joining it to the previous
    /// verbatim piece when the two are adjacent in the source.
    pub(crate) fn push_verbatim(&mut self, text: &str, source: Span) {
        if text.is_empty() {
            return;
        }
        match self.pieces.last_mut() {
            Some(last) if last.verbatim && last.source.end == source.start => {
                last.source.end = source.end;
            }
            _ => self.pieces.push(StringPiece {
                value_start: self.value.len(),
                source,
                verbatim: true,
            }),
        }
        self.value.push_str(text);
    }

    /// Appends the decoded character of an escape written at `source`.
    pub(crate) fn push_escape(&mut self, decoded: char, source: Span) {
        self.pieces.push(StringPiece {
            value_start: self.value.len(),
            source,
            verbatim: false,
        });
        self.value.push(decoded);
    }
}
