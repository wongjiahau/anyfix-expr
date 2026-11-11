//! What every generated decoder needs: spans, errors, and matching a
//! sequence of nodes. The generator copies this file into its output, so a
//! generated parser depends only on `anyfix_expr` (and on `regex`, when its
//! grammar uses `MATCHING REGEX`).

use anyfix_expr::{LineColumn, NamePart, Node, NodeKind, Span, StringLiteral, line_column};

/// A value and where it was written.
#[derive(Clone, Debug, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

/// Why a tree does not fit the grammar: what stands at `span`, and what the
/// grammar allows there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub span: Span,
    pub found: String,
    pub expected: Vec<String>,
}

impl DecodeError {
    pub fn new(span: Span, found: impl Into<String>, expected: impl Into<String>) -> Self {
        DecodeError {
            span,
            found: found.into(),
            expected: vec![expected.into()],
        }
    }

    /// `found X; expected A, B or C`, in one line. Words are listed
    /// together, and so are calls: ``the word `OF` or `OR` ``.
    pub fn message(&self) -> String {
        let mut expected = Vec::new();
        for kind in ["the word ", "the call "] {
            let same_kind = self
                .expected
                .iter()
                .filter_map(|one| one.strip_prefix(kind));
            for (index, rest) in same_kind.enumerate() {
                expected.push(match index {
                    0 => format!("{kind}{rest}"),
                    _ => rest.to_string(),
                });
            }
        }
        let others = self
            .expected
            .iter()
            .filter(|one| !one.starts_with("the word ") && !one.starts_with("the call "));
        expected.extend(others.cloned());
        let expected = match &expected[..] {
            [] => "nothing".to_string(),
            [only] => only.clone(),
            [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
        };
        format!("found {}; expected {expected}", self.found)
    }

    /// Where the error starts in `source`, as a line and column.
    pub fn position(&self, source: &str) -> LineColumn {
        line_column(source, self.span.start)
    }

    /// `file:line:column: message`, then the source line with `^` under the span.
    pub fn render(&self, source: &str, file: &str) -> String {
        let at = self.position(source);
        let line = source.lines().nth(at.line - 1).unwrap_or("");
        let width = source
            .get(self.span.start..self.span.end)
            .map_or(1, |text| text.lines().next().unwrap_or("").chars().count())
            .max(1);
        format!(
            "{file}:{at}: {}\n  {line}\n  {}{}",
            self.message(),
            " ".repeat(at.column - 1),
            "^".repeat(width)
        )
    }

    /// The more useful of two errors about the same input: the one that got
    /// further into the source, and of two starting at the same place, the
    /// one about the smaller part. Errors about the same span are joined,
    /// since every one of their expectations was possible there.
    pub fn merge(mut self, other: DecodeError) -> DecodeError {
        let rank = |error: &DecodeError| (error.span.start, std::cmp::Reverse(error.span.end));
        match rank(&self).cmp(&rank(&other)) {
            std::cmp::Ordering::Less => other,
            std::cmp::Ordering::Greater => self,
            std::cmp::Ordering::Equal => {
                for expected in other.expected {
                    if !self.expected.contains(&expected) {
                        self.expected.push(expected);
                    }
                }
                self
            }
        }
    }

    /// The next choice of a `ONE OF`, tried after this error: its result if
    /// it matches, else both errors merged.
    pub fn or<T>(self, next: Result<T, DecodeError>) -> Result<T, DecodeError> {
        next.map_err(|error| self.merge(error))
    }
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for DecodeError {}

/// How an error names the node it found.
pub fn describe(node: &Node) -> String {
    match &node.kind {
        NodeKind::Number(text) => format!("the number `{text}`"),
        NodeKind::String(literal) => {
            let mut value: String = literal.value.chars().take(40).collect();
            if value.len() < literal.value.len() {
                value.push('…');
            }
            format!("the string {value:?}")
        }
        NodeKind::Call(call) => match &call.name[..] {
            [NamePart::Word { text, .. }] => format!("the word `{text}`"),
            _ => format!("the call `{}`", call.name_text()),
        },
        NodeKind::Parens(_) => "`(...)`".to_string(),
        NodeKind::Brackets(_) => "`[...]`".to_string(),
        NodeKind::Braces(_) => "`{...}`".to_string(),
        NodeKind::Underscore => "`_`".to_string(),
        NodeKind::Missing => "nothing".to_string(),
    }
}

/// The error for a node that is not what the grammar expects there.
pub fn mismatch(node: &Node, expected: &str) -> DecodeError {
    DecodeError::new(node.span, describe(node), expected)
}

/// A sequence being matched from left to right: the elements of a bracket or
/// of the document, or the words and arguments of a call.
pub struct Sequence<'a> {
    items: Vec<Item<'a>>,
    index: usize,
    /// What the end of the sequence is called, and where it is.
    end: (&'static str, Span),
    /// Why the last repetition stopped. When the sequence then fails at the
    /// same place, this is the better explanation.
    missed: Option<DecodeError>,
}

enum Item<'a> {
    Word(&'a str, Span),
    Node(&'a Node),
}

/// The sequence of top-level elements of a document.
pub fn document(nodes: &[Node]) -> Sequence<'_> {
    let end = nodes.last().map_or(0, |last| last.span.end);
    Sequence::new(
        nodes.iter().map(Item::Node).collect(),
        ("the end of the document", Span::empty(end)),
    )
}

/// The span of a document: from its first element to its last.
pub fn document_span(nodes: &[Node]) -> Span {
    match (nodes.first(), nodes.last()) {
        (Some(first), Some(last)) => first.span.to(last.span),
        _ => Span::empty(0),
    }
}

/// The elements of `{...}`.
pub fn curly_brace(node: &Node) -> Result<Sequence<'_>, DecodeError> {
    match &node.kind {
        NodeKind::Braces(nodes) => Ok(Sequence::bracket(node, nodes, "the closing `}`")),
        _ => Err(mismatch(node, "`{...}`")),
    }
}

/// The elements of `[...]`.
pub fn square_bracket(node: &Node) -> Result<Sequence<'_>, DecodeError> {
    match &node.kind {
        NodeKind::Brackets(nodes) => Ok(Sequence::bracket(node, nodes, "the closing `]`")),
        _ => Err(mismatch(node, "`[...]`")),
    }
}

/// The elements of `(...)`.
pub fn parenthesis(node: &Node) -> Result<Sequence<'_>, DecodeError> {
    match &node.kind {
        NodeKind::Parens(nodes) => Ok(Sequence::bracket(node, nodes, "the closing `)`")),
        _ => Err(mismatch(node, "`(...)`")),
    }
}

/// The words and arguments of a call, in the order written.
pub fn call<'a>(node: &'a Node, expected: &str) -> Result<Sequence<'a>, DecodeError> {
    let NodeKind::Call(call) = &node.kind else {
        return Err(mismatch(node, expected));
    };
    let mut arguments = call.arguments.iter();
    let items = call
        .name
        .iter()
        .filter_map(|part| match part {
            NamePart::Word { text, span } => Some(Item::Word(text, *span)),
            NamePart::Slot => arguments.next().map(Item::Node),
        })
        .collect();
    Ok(Sequence::new(
        items,
        ("the end of the call", Span::empty(node.span.end)),
    ))
}

/// Any call: its name text, such as `list file in _`, and its arguments.
/// The name must pass `condition`.
pub fn any_call<'a>(
    node: &'a Node,
    condition: impl Fn(&str) -> bool,
    expected: &str,
) -> Result<(Spanned<String>, &'a [Node]), DecodeError> {
    let NodeKind::Call(call) = &node.kind else {
        return Err(mismatch(node, expected));
    };
    let name = call.name_text();
    if !condition(&name) {
        return Err(mismatch(node, expected));
    }
    let mut words = call.name.iter().filter_map(|part| match part {
        NamePart::Word { span, .. } => Some(*span),
        NamePart::Slot => None,
    });
    let span = match (words.next(), words.next_back()) {
        (Some(first), Some(last)) => first.to(last),
        (Some(only), None) => only,
        _ => node.span,
    };
    Ok((Spanned { value: name, span }, &call.arguments))
}

/// The arguments of the call `node`, as a sequence.
pub fn arguments<'a>(node: &Node, arguments: &'a [Node]) -> Sequence<'a> {
    Sequence::new(
        arguments.iter().map(Item::Node).collect(),
        ("the end of the call", Span::empty(node.span.end)),
    )
}

impl<'a> Sequence<'a> {
    fn new(items: Vec<Item<'a>>, end: (&'static str, Span)) -> Self {
        Sequence {
            items,
            index: 0,
            end,
            missed: None,
        }
    }

    fn bracket(node: &Node, nodes: &'a [Node], closer: &'static str) -> Self {
        let end = node.span.end;
        Sequence::new(
            nodes.iter().map(Item::Node).collect(),
            (closer, Span::new(end.saturating_sub(1), end)),
        )
    }

    /// The error for the item at the front not being `expected`.
    fn unexpected(&mut self, expected: &str) -> DecodeError {
        let error = match self.items.get(self.index) {
            Some(Item::Word(text, span)) => {
                DecodeError::new(*span, format!("the word `{text}`"), expected)
            }
            Some(Item::Node(node)) => mismatch(node, expected),
            None => DecodeError::new(self.end.1, self.end.0, expected),
        };
        self.fail(error)
    }

    fn fail(&mut self, error: DecodeError) -> DecodeError {
        match self.missed.take() {
            Some(missed) => missed.merge(error),
            None => error,
        }
    }

    /// Matches the word `text`: a name word of a call, or an element that is
    /// that word alone.
    pub fn word(&mut self, text: &str) -> Result<&mut Self, DecodeError> {
        self.word_span(text)?;
        Ok(self)
    }

    /// Like [`Sequence::word`], giving the word's span.
    pub fn word_span(&mut self, text: &str) -> Result<Span, DecodeError> {
        let span = match self.items.get(self.index) {
            Some(Item::Word(found, span)) if *found == text => *span,
            Some(Item::Node(node)) if is_word(node, text) => node.span,
            _ => return Err(self.unexpected(&format!("the word `{text}`"))),
        };
        self.index += 1;
        Ok(span)
    }

    /// Matches one node. `expected` names it when nothing is there.
    pub fn one<T>(
        &mut self,
        decode: impl FnOnce(&'a Node) -> Result<T, DecodeError>,
        expected: &str,
    ) -> Result<T, DecodeError> {
        let Some(Item::Node(node)) = self.items.get(self.index) else {
            return Err(self.unexpected(expected));
        };
        match decode(node) {
            Ok(value) => {
                self.index += 1;
                Ok(value)
            }
            Err(error) => Err(self.fail(error)),
        }
    }

    /// Matches one node if it is there and fits.
    pub fn optional<T>(
        &mut self,
        decode: impl FnOnce(&'a Node) -> Result<T, DecodeError>,
    ) -> Option<T> {
        let Some(Item::Node(node)) = self.items.get(self.index) else {
            return None;
        };
        match decode(node) {
            Ok(value) => {
                self.index += 1;
                Some(value)
            }
            Err(error) => {
                self.missed = Some(self.fail(error));
                None
            }
        }
    }

    /// Matches nodes for as long as they fit.
    pub fn zero_or_more<T>(
        &mut self,
        decode: impl Fn(&'a Node) -> Result<T, DecodeError>,
    ) -> Vec<T> {
        let mut values = Vec::new();
        while let Some(value) = self.optional(&decode) {
            values.push(value);
        }
        values
    }

    /// Matches one node, then more for as long as they fit.
    pub fn one_or_more<T>(
        &mut self,
        decode: impl Fn(&'a Node) -> Result<T, DecodeError>,
        expected: &str,
    ) -> Result<Vec<T>, DecodeError> {
        let mut values = vec![self.one(&decode, expected)?];
        values.extend(self.zero_or_more(decode));
        Ok(values)
    }

    /// Checks that nothing is left.
    pub fn finish(&mut self) -> Result<(), DecodeError> {
        if self.index == self.items.len() {
            return Ok(());
        }
        let end = self.end.0;
        Err(self.unexpected(end))
    }
}

fn is_word(node: &Node, expected: &str) -> bool {
    matches!(
        &node.kind,
        NodeKind::Call(call)
            if matches!(&call.name[..], [NamePart::Word { text, .. }] if text == expected)
    )
}

/// The word `text`, standing alone.
pub fn word(node: &Node, text: &str) -> Result<Span, DecodeError> {
    let mut parts = call(node, &format!("the word `{text}`"))?;
    parts.word(text)?.finish()?;
    Ok(node.span)
}

/// Any word standing alone whose text passes `condition`.
pub fn any_word(
    node: &Node,
    condition: impl Fn(&str) -> bool,
    expected: &str,
) -> Result<Spanned<String>, DecodeError> {
    if let NodeKind::Call(call) = &node.kind
        && let [NamePart::Word { text, .. }] = &call.name[..]
        && condition(text)
    {
        return Ok(Spanned {
            value: text.clone(),
            span: node.span,
        });
    }
    Err(mismatch(node, expected))
}

/// A string whose value passes `condition`.
pub fn string(
    node: &Node,
    condition: impl Fn(&str) -> bool,
    expected: &str,
) -> Result<Spanned<StringLiteral>, DecodeError> {
    match &node.kind {
        NodeKind::String(literal) if condition(&literal.value) => Ok(Spanned {
            value: literal.clone(),
            span: node.span,
        }),
        _ => Err(mismatch(node, expected)),
    }
}

/// The value of a number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumberValue {
    Integer(i128),
    Float(f64),
}

impl NumberValue {
    pub fn as_float(self) -> f64 {
        match self {
            NumberValue::Integer(value) => value as f64,
            NumberValue::Float(value) => value,
        }
    }
}

/// Reads a number's text: decimal with an optional fraction and exponent, or
/// `0x`, `0b` or `0o` and digits, with `_` between digits and an optional
/// leading `-`. A value with no fractional part is an integer however it is
/// written, so `1e3` and `1000.0` are the integer 1000.
pub fn number_value(text: &str) -> NumberValue {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text),
    };
    let digits: String = digits.chars().filter(|c| *c != '_').collect();
    let radix = match digits.as_bytes() {
        [b'0', b'x' | b'X', ..] => 16,
        [b'0', b'b' | b'B', ..] => 2,
        [b'0', b'o' | b'O', ..] => 8,
        _ => 10,
    };
    let value = if radix != 10 {
        match i128::from_str_radix(&digits[2..], radix) {
            Ok(value) => NumberValue::Integer(value),
            Err(_) => NumberValue::Float(digits[2..].chars().fold(0.0, |value, digit| {
                value * f64::from(radix) + f64::from(digit.to_digit(radix).unwrap_or(0))
            })),
        }
    } else if let Ok(value) = digits.parse::<i128>() {
        NumberValue::Integer(value)
    } else {
        let value: f64 = digits.parse().unwrap_or(f64::NAN);
        if value.fract() == 0.0 && value.abs() < 1e18 {
            NumberValue::Integer(value as i128)
        } else {
            NumberValue::Float(value)
        }
    };
    match (negative, value) {
        (false, value) => value,
        (true, NumberValue::Integer(value)) => NumberValue::Integer(-value),
        (true, NumberValue::Float(value)) => NumberValue::Float(-value),
    }
}

/// An integer from `minimum` to `maximum`.
pub fn integer<T: TryFrom<i128>>(
    node: &Node,
    minimum: i128,
    maximum: i128,
    expected: &str,
) -> Result<Spanned<T>, DecodeError> {
    if let NodeKind::Number(text) = &node.kind
        && let NumberValue::Integer(value) = number_value(text)
        && (minimum..=maximum).contains(&value)
        && let Ok(value) = T::try_from(value)
    {
        return Ok(Spanned {
            value,
            span: node.span,
        });
    }
    Err(mismatch(node, expected))
}

/// A number from `minimum` to `maximum`.
pub fn float(
    node: &Node,
    minimum: f64,
    maximum: f64,
    expected: &str,
) -> Result<Spanned<f64>, DecodeError> {
    if let NodeKind::Number(text) = &node.kind {
        let value = number_value(text).as_float();
        if minimum <= value && value <= maximum {
            return Ok(Spanned {
                value,
                span: node.span,
            });
        }
    }
    Err(mismatch(node, expected))
}
