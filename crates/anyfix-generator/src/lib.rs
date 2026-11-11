//! A parser generator for anyfix expressions. It reads a grammar written in
//! anyfix and writes Rust: a typed AST, plus a decoder that turns the tree
//! from `anyfix_expr::SyntaxTree::nodes()` into that AST. See `README.md`
//! for the grammar language.

mod analyze;
pub mod diagnostic;
pub mod grammar;
mod lower;
pub mod model;
mod overlap;

pub use diagnostic::{Diagnostic, Severity};

/// A grammar that passed every check, and the warnings about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    pub grammar: model::Grammar,
    pub warnings: Vec<Diagnostic>,
}

/// Reads and checks the grammar in `source`. The errors, when there are
/// any, are sorted by position.
pub fn check(source: &str) -> Result<Checked, Vec<Diagnostic>> {
    let parsed = anyfix_expr::parse(source);
    if !parsed.errors.is_empty() {
        return Err(parsed.errors.into_iter().map(Diagnostic::from).collect());
    }
    let document = grammar::decode_document(&parsed.syntax.nodes())
        .map_err(|error| vec![Diagnostic::from(error)])?;
    let (grammar, mut diagnostics) = lower::lower(&document, source);
    if diagnostics.is_empty() {
        diagnostics = analyze::analyze(&grammar);
    }
    diagnostics.sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.span.end));
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(diagnostics);
    }
    Ok(Checked {
        grammar,
        warnings: diagnostics,
    })
}
