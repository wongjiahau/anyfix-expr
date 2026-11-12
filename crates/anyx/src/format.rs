//! The canonical formatter.
//!
//! It works in two passes. The first deletes every `.`, `:` and pair of
//! grouping `( )` whose removal leaves the tree unchanged, trying each one and
//! re-parsing the element it sits in to check. The second lays the remaining
//! tokens out at [`WIDTH`] columns, with comments attached where they were.

use std::collections::HashMap;

use crate::error::Error;
use crate::lexer::is_symbol_word;
use crate::node::{NamePart, Node, NodeKind};
use crate::parser::parse;
use crate::span::Span;
use crate::syntax::{
    Expression, Group, GroupKind, Item, Sequence, SyntaxTree, Token, TokenKind, TriviaKind,
};

/// The line width the layout aims for.
pub const WIDTH: usize = 100;
const INDENT: usize = 4;

/// Formats a document canonically, or returns its parse errors.
///
/// A `.`, `:` or `( )` is removed only when the tree stays the same, so
/// `1 . f` becomes `1 f` but `x . f` stays (`x f` is another call). The
/// result has the same tree as the input, keeps every comment, and formats to
/// itself.
///
/// ```
/// let formatted = anyx::format("f : 1 . g, ((x)) y").unwrap();
/// assert_eq!(formatted, "f: 1 g,\n(x) y,\n");
/// ```
pub fn format(source: &str) -> Result<String, Vec<Error>> {
    let parsed = parse(source);
    if !parsed.errors.is_empty() {
        return Err(parsed.errors);
    }
    let original = Walk::new(&parsed.syntax);
    let mut kept = vec![true; original.tokens.len()];
    for &id in &original.top {
        original.simplify(id, &mut kept);
    }
    let kept_ids: Vec<usize> = (0..kept.len()).filter(|&i| kept[i]).collect();
    let reparsed = parse(&join(kept_ids.iter().map(|&i| original.tokens[i])));
    assert!(
        reparsed.errors.is_empty(),
        "removing redundant tokens broke {source:?}"
    );
    let simplified = Walk::new(&reparsed.syntax);
    assert_eq!(simplified.tokens.len(), kept_ids.len());
    let comments = Comments::new(source, &original.tokens, &kept_ids);
    let mut builder = Builder {
        index: simplified
            .tokens
            .iter()
            .enumerate()
            .map(|(k, token)| (token.span.start, k))
            .collect(),
        comments: &comments,
        start: None,
    };
    let output = print(&builder.document(&reparsed.syntax));
    let check = parse(&output);
    assert!(
        check.errors.is_empty() && same_nodes(&check.syntax.nodes(), &parsed.syntax.nodes()),
        "the formatter changed the tree of {source:?} into {output:?}"
    );
    Ok(output)
}

/// Token texts joined by single spaces: a source with the same tokens, since
/// whitespace never merges or splits them.
fn join<'a>(tokens: impl Iterator<Item = &'a Token>) -> String {
    tokens
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Pass 1: removing redundant tokens

/// The tokens of a tree in source order, the end token last, and the elements
/// they make up.
struct Walk<'a> {
    tokens: Vec<&'a Token>,
    elements: Vec<ElementInfo>,
    top: Vec<usize>,
}

struct ElementInfo {
    /// The element's tokens, not counting its comma.
    tokens: std::ops::Range<usize>,
    /// Sets of tokens that may be redundant: each `.` and `:`, and each pair
    /// of parens around one element with no comma.
    candidates: Vec<Vec<usize>>,
    /// The elements inside its brackets.
    children: Vec<usize>,
}

impl<'a> Walk<'a> {
    fn new(tree: &'a SyntaxTree) -> Self {
        let mut walk = Walk {
            tokens: Vec::new(),
            elements: Vec::new(),
            top: Vec::new(),
        };
        walk.top = walk.sequence(&tree.sequence);
        walk.tokens.push(&tree.end);
        walk
    }

    fn sequence(&mut self, sequence: &'a Sequence) -> Vec<usize> {
        let mut ids = Vec::new();
        for element in &sequence.elements {
            let start = self.tokens.len();
            let mut candidates = Vec::new();
            let mut children = Vec::new();
            self.expression(&element.expression, &mut candidates, &mut children);
            ids.push(self.elements.len());
            self.elements.push(ElementInfo {
                tokens: start..self.tokens.len(),
                candidates,
                children,
            });
            if let Some(comma) = &element.comma {
                self.tokens.push(comma);
            }
        }
        ids
    }

    fn expression(
        &mut self,
        expression: &'a Expression,
        candidates: &mut Vec<Vec<usize>>,
        children: &mut Vec<usize>,
    ) {
        match expression {
            Expression::Run(run) => {
                for item in &run.items {
                    match item {
                        Item::Atom(token) | Item::Skipped(token) => self.tokens.push(token),
                        Item::Group(group) => {
                            let open = self.tokens.len();
                            self.tokens.push(&group.open);
                            children.extend(self.sequence(&group.sequence));
                            if let Some(close) = &group.close {
                                let groups_one = matches!(
                                    &group.sequence.elements[..],
                                    [element] if element.comma.is_none()
                                );
                                if group.kind == GroupKind::Parens && groups_one {
                                    candidates.push(vec![open, self.tokens.len()]);
                                }
                                self.tokens.push(close);
                            }
                        }
                    }
                }
            }
            Expression::Dot(binary) | Expression::Colon(binary) => {
                self.expression(&binary.left, candidates, children);
                candidates.push(vec![self.tokens.len()]);
                self.tokens.push(&binary.operator);
                self.expression(&binary.right, candidates, children);
            }
        }
    }

    /// Deletes the redundant tokens of element `id`, innermost elements first.
    /// A candidate is deleted when the element re-parses to the same tree
    /// without it. Deleting one can make another redundant, so the candidates
    /// are retried until none is deleted.
    fn simplify(&self, id: usize, kept: &mut [bool]) {
        let element = &self.elements[id];
        for &child in &element.children {
            self.simplify(child, kept);
        }
        let reparse = |kept: &[bool]| {
            let tokens = element.tokens.clone().filter(|&i| kept[i]);
            parse(&join(tokens.map(|i| self.tokens[i])))
        };
        let target = reparse(kept).syntax.nodes();
        loop {
            let mut changed = false;
            for candidate in &element.candidates {
                if !candidate.iter().all(|&i| kept[i]) {
                    continue;
                }
                candidate.iter().for_each(|&i| kept[i] = false);
                let parsed = reparse(kept);
                if parsed.errors.is_empty() && same_nodes(&parsed.syntax.nodes(), &target) {
                    changed = true;
                } else {
                    candidate.iter().for_each(|&i| kept[i] = true);
                }
            }
            if !changed {
                break;
            }
        }
    }
}

fn same_nodes(a: &[Node], b: &[Node]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_node(a, b))
}

/// Whether two nodes are the same tree, ignoring spans.
fn same_node(a: &Node, b: &Node) -> bool {
    match (&a.kind, &b.kind) {
        (NodeKind::Number(a), NodeKind::Number(b)) => a == b,
        (NodeKind::String(a), NodeKind::String(b)) => a.value == b.value,
        (NodeKind::Call(a), NodeKind::Call(b)) => {
            a.name.len() == b.name.len()
                && a.name.iter().zip(&b.name).all(|parts| match parts {
                    (NamePart::Word { text: a, .. }, NamePart::Word { text: b, .. }) => a == b,
                    (NamePart::Slot, NamePart::Slot) => true,
                    _ => false,
                })
                && same_nodes(&a.arguments, &b.arguments)
        }
        (NodeKind::Parens(a), NodeKind::Parens(b))
        | (NodeKind::Brackets(a), NodeKind::Brackets(b))
        | (NodeKind::Braces(a), NodeKind::Braces(b)) => same_nodes(a, b),
        (NodeKind::Underscore, NodeKind::Underscore) | (NodeKind::Missing, NodeKind::Missing) => {
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Comments

/// Every comment of the original source, attached to a kept token by
/// position. Indexed by the kept token's position in the kept sequence.
struct Comments {
    /// Comments on their own lines before the token.
    leading: Vec<Vec<Comment>>,
    /// The comment after the token on the same line.
    trailing: Vec<Option<String>>,
    /// Whether a blank line came before the token.
    blank: Vec<bool>,
}

struct Comment {
    text: String,
    /// Whether a blank line came before the comment.
    blank: bool,
}

impl Comments {
    /// A comment on the same line as the kept token before it trails that
    /// token. Any other comment leads the next kept token. Comments on deleted
    /// tokens move this way too.
    fn new(source: &str, tokens: &[&Token], kept_ids: &[usize]) -> Self {
        let spans: Vec<Span> = kept_ids.iter().map(|&i| tokens[i].span).collect();
        let found: Vec<(&str, Span)> = tokens
            .iter()
            .flat_map(|token| token.leading.iter().chain(&token.trailing))
            .filter(|trivia| trivia.kind == TriviaKind::Comment)
            .map(|trivia| (trivia.text.as_str(), trivia.span))
            .collect();

        let mut blank = vec![false; spans.len()];
        let mut comment_blank = vec![false; found.len()];
        let (mut k, mut j, mut previous_end) = (0, 0, None);
        while k < spans.len() || j < found.len() {
            let comment =
                j < found.len() && (k == spans.len() || found[j].1.start < spans[k].start);
            let span = if comment { found[j].1 } else { spans[k] };
            let is_blank =
                previous_end.is_some_and(|end| source[end..span.start].matches('\n').count() >= 2);
            if comment {
                comment_blank[j] = is_blank;
                j += 1;
            } else {
                blank[k] = is_blank;
                k += 1;
            }
            previous_end = Some(span.end);
        }

        let mut leading: Vec<Vec<Comment>> = spans.iter().map(|_| Vec::new()).collect();
        let mut trailing = vec![None; spans.len()];
        for ((text, span), blank) in found.into_iter().zip(comment_blank) {
            let next = spans.partition_point(|token| token.start < span.start);
            match next.checked_sub(1) {
                Some(previous) if !source[spans[previous].end..span.start].contains('\n') => {
                    trailing[previous] = Some(text.to_string());
                }
                _ => leading[next].push(Comment {
                    text: text.to_string(),
                    blank,
                }),
            }
        }
        // A comment on its own line before a comma belongs with the next
        // element. So does one after a comma whose element already has a
        // trailing comment, since a line holds only one.
        for k in 0..spans.len() {
            if tokens[kept_ids[k]].kind != TokenKind::Comma {
                continue;
            }
            let mut moved = std::mem::take(&mut leading[k]);
            if k > 0
                && trailing[k - 1].is_some()
                && let Some(text) = trailing[k].take()
            {
                moved.push(Comment { text, blank: false });
            }
            moved.append(&mut leading[k + 1]);
            leading[k + 1] = moved;
        }
        Comments {
            leading,
            trailing,
            blank,
        }
    }
}

// ---------------------------------------------------------------------------
// Pass 2: layout

#[derive(Debug)]
enum Doc {
    Text(String),
    /// A space, or a line break when its group breaks.
    Line,
    /// Nothing, or a line break when its group breaks.
    SoftLine,
    HardLine,
    Indent(Box<Doc>),
    Concat(Vec<Doc>),
    Group {
        doc: Box<Doc>,
        /// The group must break.
        force: bool,
        /// It holds a hard line break, so every enclosing group breaks too.
        hard: bool,
        /// It holds a trailing comment, so every enclosing bracket breaks too.
        suffix: bool,
    },
    /// A comma. Unlike other text, it may follow a trailing comment on the
    /// same line, since the comment is printed after it.
    Punct(&'static str),
    /// A comma printed only when its group breaks.
    IfBreak(&'static str),
    /// A trailing comment, printed at the end of the line.
    LineSuffix(String),
    /// A raw string: its opening quotes and the lines of its value, indented
    /// one level past the current line.
    Raw {
        quotes: String,
        lines: Vec<String>,
    },
}

fn indent(doc: Doc) -> Doc {
    Doc::Indent(Box::new(doc))
}

/// Whether `doc` holds a hard line break and whether it holds a trailing
/// comment, outside the groups it holds (whose own flags count instead).
fn holds(doc: &Doc) -> (bool, bool) {
    match doc {
        Doc::HardLine => (true, false),
        // A raw string breaks the brackets around it, like a trailing comment.
        Doc::LineSuffix(_) | Doc::Raw { .. } => (false, true),
        Doc::Indent(doc) => holds(doc),
        Doc::Concat(docs) => docs
            .iter()
            .map(holds)
            .fold((false, false), |(a, b), (c, d)| (a || c, b || d)),
        Doc::Group { hard, suffix, .. } => (*hard, *suffix),
        _ => (false, false),
    }
}

/// A bracketed sequence, which breaks when it holds a comment, or when `force`.
fn bracket_group(doc: Doc, force: bool) -> Doc {
    let (hard, suffix) = holds(&doc);
    Doc::Group {
        doc: Box::new(doc),
        force: force || hard || suffix,
        hard: hard || force,
        suffix,
    }
}

/// A `.` chain. A trailing comment inside moves to the end of its line
/// instead of breaking the chain.
fn chain_group(doc: Doc) -> Doc {
    let (hard, suffix) = holds(&doc);
    Doc::Group {
        doc: Box::new(doc),
        force: hard,
        hard,
        suffix,
    }
}

struct Builder<'a> {
    /// A kept token's index, by where it starts in the simplified source.
    index: HashMap<usize, usize>,
    comments: &'a Comments,
    /// Set at the start of an element, holding whether a blank line may come
    /// before it. Taken by the element's first token.
    start: Option<bool>,
}

impl Builder<'_> {
    fn document(&mut self, tree: &SyntaxTree) -> Doc {
        let elements = &tree.sequence.elements;
        let mut parts = Vec::new();
        for (i, element) in elements.iter().enumerate() {
            if i > 0 {
                parts.push(Doc::HardLine);
            }
            self.start = Some(i > 0);
            parts.push(self.expression(&element.expression));
            if elements.len() > 1 {
                parts.push(Doc::Punct(","));
            }
            if let Some(comma) = &element.comma {
                parts.push(self.comma_comments(comma));
            }
        }
        let end = self.index[&tree.end.span.start];
        for (j, comment) in self.comments.leading[end].iter().enumerate() {
            if !elements.is_empty() || j > 0 {
                parts.push(Doc::HardLine);
                if comment.blank {
                    parts.push(Doc::HardLine);
                }
            }
            parts.push(Doc::Text(comment.text.clone()));
        }
        Doc::Concat(parts)
    }

    fn expression(&mut self, expression: &Expression) -> Doc {
        match expression {
            Expression::Run(run) => {
                let mut parts = Vec::new();
                for (i, item) in run.items.iter().enumerate() {
                    if i > 0 {
                        parts.push(Doc::Text(" ".into()));
                    }
                    parts.push(match item {
                        Item::Atom(token) => self.token(token),
                        Item::Group(group) => self.group(group),
                        Item::Skipped(_) => unreachable!("a document with errors is not formatted"),
                    });
                }
                Doc::Concat(parts)
            }
            Expression::Dot(_) => {
                let mut links = Vec::new();
                let mut first = expression;
                while let Expression::Dot(binary) = first {
                    links.push(binary);
                    first = &binary.left;
                }
                let first = self.expression(first);
                let mut rest = Vec::new();
                for binary in links.into_iter().rev() {
                    rest.push(Doc::Line);
                    rest.push(self.token(&binary.operator));
                    if !starts_with_symbol_word(&binary.right) {
                        rest.push(Doc::Text(" ".into()));
                    }
                    rest.push(self.expression(&binary.right));
                }
                chain_group(Doc::Concat(vec![first, indent(Doc::Concat(rest))]))
            }
            Expression::Colon(_) => {
                let mut parts = Vec::new();
                let mut current = expression;
                while let Expression::Colon(binary) = current {
                    parts.push(self.expression(&binary.left));
                    parts.push(self.token(&binary.operator));
                    parts.push(Doc::Text(" ".into()));
                    current = &binary.right;
                }
                parts.push(self.expression(current));
                Doc::Concat(parts)
            }
        }
    }

    fn group(&mut self, group: &Group) -> Doc {
        let open = self.token(&group.open);
        let close = group
            .close
            .as_ref()
            .expect("a document with errors is not formatted");
        let close_index = self.index[&close.span.start];
        let close_comments = &self.comments.leading[close_index];
        let close_doc = Doc::Text(close.text.clone());
        let close_trailing = self.trailing(close_index);
        let elements = &group.sequence.elements;
        if elements.is_empty() && close_comments.is_empty() {
            return Doc::Concat(vec![open, close_doc, close_trailing]);
        }

        let mut inner = Vec::new();
        for (i, element) in elements.iter().enumerate() {
            self.start = Some(i > 0);
            inner.push(self.expression(&element.expression));
            let last = i + 1 == elements.len();
            // In parens, one element's comma decides the tree: `(x,)` is
            // `Parens[x]` but `(x)` is `x`. So it stays exactly as written.
            let lone_parens = group.kind == GroupKind::Parens && elements.len() == 1;
            inner.push(if lone_parens {
                Doc::Punct(if element.comma.is_some() { "," } else { "" })
            } else if !last || !close_comments.is_empty() {
                Doc::Punct(",")
            } else {
                Doc::IfBreak(",")
            });
            if let Some(comma) = &element.comma {
                inner.push(self.comma_comments(comma));
            }
            if !last {
                inner.push(Doc::Line);
            }
        }
        for (j, comment) in close_comments.iter().enumerate() {
            if !elements.is_empty() || j > 0 {
                inner.push(Doc::HardLine);
                if comment.blank {
                    inner.push(Doc::HardLine);
                }
            }
            inner.push(Doc::Text(comment.text.clone()));
        }

        let pad = || match group.kind {
            GroupKind::Braces => Doc::Line,
            GroupKind::Parens | GroupKind::Brackets => Doc::SoftLine,
        };
        let mut body = vec![pad()];
        body.extend(inner);
        let group = bracket_group(
            Doc::Concat(vec![open, indent(Doc::Concat(body)), pad(), close_doc]),
            !close_comments.is_empty(),
        );
        Doc::Concat(vec![group, close_trailing])
    }

    /// A token with its comments. At the start of an element, leading comments
    /// sit on their own lines above it. Elsewhere they break the line and
    /// continue one level in.
    fn token(&mut self, token: &Token) -> Doc {
        let k = self.index[&token.span.start];
        let leading = &self.comments.leading[k];
        let mut parts = Vec::new();
        match self.start.take() {
            Some(mut allow_blank) => {
                for comment in leading {
                    if allow_blank && comment.blank {
                        parts.push(Doc::HardLine);
                    }
                    parts.push(Doc::Text(comment.text.clone()));
                    parts.push(Doc::HardLine);
                    allow_blank = true;
                }
                if allow_blank && self.comments.blank[k] {
                    parts.push(Doc::HardLine);
                }
            }
            None if !leading.is_empty() => {
                for comment in leading {
                    parts.push(indent(Doc::HardLine));
                    parts.push(Doc::Text(comment.text.clone()));
                }
                parts.push(indent(Doc::HardLine));
            }
            None => {}
        }
        parts.push(token_text(token));
        parts.push(self.trailing(k));
        Doc::Concat(parts)
    }

    fn trailing(&self, k: usize) -> Doc {
        match &self.comments.trailing[k] {
            Some(text) => Doc::LineSuffix(format!(" {text}")),
            None => Doc::Concat(Vec::new()),
        }
    }

    /// A comma's comments, which all go to the end of its line, since the
    /// comma itself is printed by the layout.
    fn comma_comments(&self, comma: &Token) -> Doc {
        let k = self.index[&comma.span.start];
        let mut parts: Vec<Doc> = self.comments.leading[k]
            .iter()
            .map(|comment| Doc::LineSuffix(format!(" {}", comment.text)))
            .collect();
        parts.push(self.trailing(k));
        Doc::Concat(parts)
    }
}

/// A `.` hugs a symbol word after it: `1 .+ 2`.
fn starts_with_symbol_word(expression: &Expression) -> bool {
    matches!(
        expression,
        Expression::Run(run) if matches!(
            run.items.first(),
            Some(Item::Atom(token)) if token.kind == TokenKind::Word && is_symbol_word(&token.text)
        )
    )
}

fn token_text(token: &Token) -> Doc {
    match &token.kind {
        TokenKind::String(literal) if token.text.starts_with("\"\"\"") => Doc::Raw {
            quotes: token.text.chars().take_while(|&c| c == '"').collect(),
            lines: if literal.value.is_empty() {
                Vec::new()
            } else {
                literal.value.split('\n').map(str::to_string).collect()
            },
        },
        _ => Doc::Text(token.text.clone()),
    }
}

// ---------------------------------------------------------------------------
// Printing

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

fn print(doc: &Doc) -> String {
    let mut printer = Printer {
        out: String::new(),
        column: 0,
        line_indent: 0,
        continuation: false,
        suffix: Vec::new(),
    };
    let mut stack = vec![(0, Mode::Break, doc)];
    while let Some((level, mode, doc)) = stack.pop() {
        match doc {
            Doc::Text(text) => printer.text(text),
            Doc::Line if mode == Mode::Flat => printer.text(" "),
            Doc::SoftLine if mode == Mode::Flat => {}
            Doc::Line | Doc::SoftLine | Doc::HardLine => printer.newline(level),
            Doc::Indent(doc) => stack.push((level + INDENT, mode, doc)),
            Doc::Concat(docs) => stack.extend(docs.iter().rev().map(|doc| (level, mode, doc))),
            Doc::Group { doc, force, .. } => {
                let width = WIDTH as isize - printer.column as isize;
                let mode = if *force {
                    Mode::Break
                } else if mode == Mode::Flat || fits(doc, width, &stack) {
                    Mode::Flat
                } else {
                    Mode::Break
                };
                stack.push((level, mode, doc));
            }
            Doc::Punct(text) => printer.punct(text),
            Doc::IfBreak(text) => {
                if mode == Mode::Break {
                    printer.punct(text);
                }
            }
            Doc::LineSuffix(text) => printer.suffix.push(text.clone()),
            Doc::Raw { quotes, lines } => printer.raw(quotes, lines),
        }
    }
    printer.finish()
}

/// Whether `doc` fits flat in `width` columns, along with what follows it up
/// to the next line break.
fn fits(doc: &Doc, mut width: isize, rest: &[(usize, Mode, &Doc)]) -> bool {
    let mut stack = vec![(Mode::Flat, doc)];
    let mut rest = rest.iter().rev();
    loop {
        let (mode, doc) = match stack.pop() {
            Some(next) => next,
            None => match rest.next() {
                Some(&(_, mode, doc)) => (mode, doc),
                None => return true,
            },
        };
        match doc {
            Doc::Text(text) => match text.split_once('\n') {
                Some((line, _)) => return width >= line.chars().count() as isize,
                None => width -= text.chars().count() as isize,
            },
            Doc::Line if mode == Mode::Flat => width -= 1,
            Doc::SoftLine if mode == Mode::Flat => {}
            Doc::Line | Doc::SoftLine | Doc::HardLine => return true,
            Doc::Indent(doc) => stack.push((mode, doc)),
            Doc::Concat(docs) => stack.extend(docs.iter().rev().map(|doc| (mode, doc))),
            Doc::Group { doc, force, .. } => {
                stack.push((if *force { Mode::Break } else { mode }, doc));
            }
            Doc::Punct(text) => width -= text.len() as isize,
            Doc::IfBreak(text) => {
                if mode == Mode::Break {
                    width -= text.len() as isize;
                }
            }
            Doc::LineSuffix(_) => {}
            Doc::Raw { quotes, .. } => return width >= quotes.len() as isize,
        }
        if width < 0 {
            return false;
        }
    }
}

struct Printer {
    out: String,
    column: usize,
    /// The indentation the current line started with.
    line_indent: usize,
    /// Whether the current line continues the one before, after a comment.
    continuation: bool,
    /// Trailing comments waiting for the end of the line.
    suffix: Vec<String>,
}

impl Printer {
    /// Text after a trailing comment would be swallowed by it, so the comment
    /// ends its line first and the text continues one level in.
    fn text(&mut self, text: &str) {
        if !self.suffix.is_empty() && !text.trim().is_empty() {
            self.continue_line();
        }
        self.out.push_str(text);
        self.column = match text.rsplit_once('\n') {
            Some((_, last)) => last.chars().count(),
            None => self.column + text.chars().count(),
        };
    }

    fn punct(&mut self, text: &str) {
        self.out.push_str(text);
        self.column += text.len();
    }

    fn newline(&mut self, level: usize) {
        self.end_line();
        self.out.push('\n');
        self.out.push_str(&" ".repeat(level));
        self.column = level;
        self.line_indent = level;
        self.continuation = false;
    }

    /// Breaks the line one level past the line it continues.
    fn continue_line(&mut self) {
        let level = if self.continuation {
            self.line_indent
        } else {
            self.line_indent + INDENT
        };
        self.newline(level);
        self.continuation = true;
    }

    fn end_line(&mut self) {
        self.trim();
        for text in self.suffix.drain(..) {
            self.out.push_str(&text);
        }
        self.trim();
    }

    fn trim(&mut self) {
        let trimmed = self.out.trim_end_matches(' ').len();
        self.out.truncate(trimmed);
    }

    /// The content lines and the closing line sit one level past the line
    /// the string opens on. Nothing may follow the opening quotes, so a
    /// waiting comment goes first.
    fn raw(&mut self, quotes: &str, lines: &[String]) {
        if !self.suffix.is_empty() {
            self.continue_line();
        }
        self.line_indent += INDENT;
        let inner = " ".repeat(self.line_indent);
        self.out.push_str(quotes);
        for line in lines {
            self.out.push('\n');
            if !line.is_empty() {
                self.out.push_str(&inner);
                self.out.push_str(line);
            }
        }
        self.out.push('\n');
        self.out.push_str(&inner);
        self.out.push_str(quotes);
        self.column = self.line_indent + quotes.len();
    }

    fn finish(mut self) -> String {
        self.end_line();
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        self.out
    }
}
