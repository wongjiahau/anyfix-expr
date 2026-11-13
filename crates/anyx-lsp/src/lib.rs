//! A language server for any anyx grammar. It needs nothing written for the
//! grammar by hand: the decoder anyx-generate writes is enough.
//!
//! ```ignore
//! mod generated; // written by `anyx-generate grammar.anyx`
//!
//! anyx_lsp::language!(MyLanguage = generated);
//!
//! fn main() {
//!     anyx_lsp::serve(&MyLanguage).unwrap();
//! }
//! ```
//!
//! What it offers, all from the grammar:
//!
//! - diagnostics: anyx's syntax errors, and every error the decoder kept
//!   (one per element of an `EACH` list, so one mistake does not hide the rest);
//! - completion: the words and calls the grammar allows where the cursor is;
//! - formatting, with the grammar's `FORMAT` layouts;
//! - folding of brackets and strings that span lines.
//!
//! [`serve`] runs it over standard input and output, for editors. With the
//! `wasm` feature, [`wasm!`] exports it to a web page, which runs it in a Web
//! Worker: the page needs no server.

mod position;
mod server;
#[cfg(feature = "wasm")]
pub mod wasm;

pub use position::LineIndex;
pub use server::{Server, serve};

use anyx::{Node, NodeKind, Span};

/// What a document gets wrong, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub span: Span,
    pub message: String,
    /// What the grammar allows at `span`, as the decoder describes it:
    /// ``the word `notes` ``, ``the call `metadata _` ``, `a string`.
    pub expected: Vec<String>,
    pub kind: ProblemKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    /// anyx itself could not read it, such as an unclosed bracket.
    Syntax,
    /// It is anyx, but not what the grammar describes.
    Grammar,
}

impl Problem {
    pub fn syntax(error: &anyx::Error) -> Problem {
        Problem {
            span: error.span,
            message: error.message.clone(),
            expected: Vec::new(),
            kind: ProblemKind::Syntax,
        }
    }
}

/// A grammar the server serves. [`language!`] writes it for a generated
/// decoder.
pub trait Language {
    /// Every problem in `source`, syntax errors first.
    fn check(&self, source: &str) -> Vec<Problem>;

    /// `source` laid out canonically, or `None` when it has syntax errors.
    fn format(&self, source: &str) -> Option<String>;
}

/// Writes a [`Language`] named `$name` for the generated decoder in the
/// module `$module`.
#[macro_export]
macro_rules! language {
    ($name:ident = $($module:ident)::+) => {
        pub struct $name;

        impl $crate::Language for $name {
            fn check(&self, source: &str) -> ::std::vec::Vec<$crate::Problem> {
                let parsed = $($module)::+::parse_document(source);
                let mut problems: ::std::vec::Vec<$crate::Problem> =
                    parsed.syntax_errors.iter().map($crate::Problem::syntax).collect();
                for error in $($module)::+::support::Errors::all_errors(&parsed.document) {
                    problems.push($crate::Problem {
                        span: error.span,
                        message: error.message(),
                        expected: error.expected.clone(),
                        kind: $crate::ProblemKind::Grammar,
                    });
                }
                problems
            }

            fn format(&self, source: &str) -> ::std::option::Option<::std::string::String> {
                $($module)::+::format(source).ok()
            }
        }
    };
}

/// A word put where the cursor is, so that the decoder says what it expected
/// there. It is a plain lowercase word, which no grammar is likely to name.
const PROBE: &str = "zzzcompletionzzz";

/// One thing that may be written at the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// The words, as a menu shows them: `metadata`, `could not convert`.
    pub label: String,
    /// A snippet: the words, with `$1`, `$2`… for the call's slots.
    pub snippet: String,
    /// Where the expectation came from: ``the call `metadata _` ``.
    pub detail: String,
}

/// What the grammar allows at `offset`: the words and calls the decoder
/// expected there, found by writing a probe word over the word being typed
/// and decoding again.
pub fn complete(language: &dyn Language, source: &str, offset: usize) -> Vec<Completion> {
    let offset = offset.min(source.len());
    let start = source[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, character)| character.is_alphanumeric() || *character == '_')
        .last()
        .map_or(offset, |(index, _)| index);
    let probed = format!("{}{PROBE}{}", &source[..start], &source[offset..]);
    let probe = Span::new(start, start + PROBE.len());
    let innermost = language
        .check(&probed)
        .into_iter()
        .filter(|problem| problem.kind == ProblemKind::Grammar)
        .filter(|problem| problem.span.start <= probe.start && probe.end <= problem.span.end)
        .min_by_key(|problem| problem.span.end - problem.span.start);
    let Some(problem) = innermost else {
        return Vec::new();
    };
    let mut completions: Vec<Completion> = Vec::new();
    for expected in &problem.expected {
        if let Some(mut completion) = completion(expected) {
            completion.snippet = without_typed_words(&source[..start], &completion.snippet);
            if !completions
                .iter()
                .any(|known| known.snippet == completion.snippet)
            {
                completions.push(completion);
            }
        }
    }
    completions
}

/// `snippet` without its first words when they are already written just
/// before the cursor: after `max `, `max measures per system` is
/// `measures per system`.
fn without_typed_words(before: &str, snippet: &str) -> String {
    let words: Vec<&str> = snippet.split(' ').collect();
    for typed in (1..words.len()).rev() {
        let prefix = format!("{} ", words[..typed].join(" "));
        if let Some(rest) = before.strip_suffix(&prefix)
            && !rest.ends_with(|character: char| character.is_alphanumeric() || character == '_')
        {
            return words[typed..].join(" ");
        }
    }
    snippet.to_string()
}

/// A completion for one expectation, when it starts with words.
fn completion(expected: &str) -> Option<Completion> {
    let quoted = |prefix: &str| {
        expected
            .strip_prefix(prefix)?
            .strip_prefix('`')?
            .strip_suffix('`')
            .map(str::to_string)
    };
    if let Some(word) = quoted("the word ") {
        return Some(Completion {
            label: word.clone(),
            snippet: word,
            detail: expected.to_string(),
        });
    }
    let call = quoted("the call ")?;
    // A call that starts with a slot is written after something else, which
    // the cursor is not after.
    if call.starts_with('_') {
        return None;
    }
    let mut slot = 0;
    let parts: Vec<String> = call
        .split(' ')
        .map(|part| match part {
            "_" => {
                slot += 1;
                format!("${slot}")
            }
            word => word.to_string(),
        })
        .collect();
    let label = call
        .split(' ')
        .filter(|part| *part != "_")
        .collect::<Vec<_>>()
        .join(" ");
    Some(Completion {
        label,
        snippet: parts.join(" "),
        detail: expected.to_string(),
    })
}

/// The brackets and strings in `source` that span more than one line, as
/// byte spans, outermost first.
pub fn folds(source: &str) -> Vec<Span> {
    fn visit(node: &Node, found: &mut Vec<Span>) {
        let children: &[Node] = match &node.kind {
            NodeKind::Parens(nodes) | NodeKind::Brackets(nodes) | NodeKind::Braces(nodes) => {
                found.push(node.span);
                nodes
            }
            NodeKind::String(_) => {
                found.push(node.span);
                &[]
            }
            NodeKind::Call(call) => &call.arguments,
            NodeKind::Number(_) | NodeKind::Underscore | NodeKind::Missing => &[],
        };
        for child in children {
            visit(child, found);
        }
    }
    let mut found = Vec::new();
    for node in anyx::parse(source).syntax.nodes() {
        visit(&node, &mut found);
    }
    found
        .into_iter()
        .filter(|span| source[span.start..span.end].contains('\n'))
        .collect()
}
