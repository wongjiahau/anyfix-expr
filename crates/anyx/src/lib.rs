//! Anyx expressions: a notation that only describes grouping.
//! It gives nothing a meaning; that is the consumer's job. See `README.md`
//! for the specification.
//!
//! ```
//! let parsed = anyx::parse("search { max depth : 3 }");
//! assert!(parsed.errors.is_empty());
//! let nodes = parsed.syntax.nodes();
//! let anyx::NodeKind::Call(call) = &nodes[0].kind else { unreachable!() };
//! assert_eq!(call.name_text(), "search _");
//! assert_eq!(parsed.syntax.to_source(), "search { max depth : 3 }");
//! ```
//!
//! [`parse`] returns a lossless [`SyntaxTree`] (printing it back reproduces
//! the input byte for byte) and every [`Error`] found. [`SyntaxTree::nodes`]
//! derives the abstract [`Node`] view from it, where every run is a [`Call`].

mod error;
mod format;
mod lexer;
mod migrate;
mod node;
mod parser;
mod span;
mod string;
mod syntax;

pub use error::{Error, ErrorCode};
pub use format::{Layout, WIDTH, format, format_with};
pub use lexer::{is_keyword, is_symbol_word};
pub use migrate::migrate_comments;
pub use node::{Call, FlatWord, FlatWordKind, NamePart, Node, NodeKind, flat_words};
pub use parser::{MAX_DEPTH, Parsed, parse};
pub use span::{LineColumn, Span, line_column};
pub use string::{StringLiteral, StringPiece};
pub use syntax::{
    BinaryExpression, Element, Expression, Group, GroupKind, Item, Run, Sequence, SyntaxTree,
    Token, TokenKind, Trivia, TriviaKind,
};
