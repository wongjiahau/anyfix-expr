//! A parser generator for anyx expressions. It reads a grammar written in
//! anyx and writes Rust: a typed AST, plus a decoder that turns the tree
//! from `anyx::SyntaxTree::nodes()` into that AST. See `README.md`
//! for the grammar language.

mod analyze;
pub mod diagnostic;
mod generate;
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
    let parsed = anyx::parse(source);
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

/// The Rust written for a grammar, and the warnings about the grammar.
#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    /// One file: the types, their decoders and the support module. It is
    /// valid Rust but not laid out; see [`format`].
    pub code: String,
    pub warnings: Vec<Diagnostic>,
}

/// Checks the grammar in `source` and writes its Rust. The same grammar
/// always gives the same text.
pub fn generate(source: &str) -> Result<Generated, Vec<Diagnostic>> {
    let checked = check(source)?;
    let code = generate::generate(&checked.grammar)?;
    Ok(Generated {
        code,
        warnings: checked.warnings,
    })
}

/// Lays `code` out with `rustfmt`, which must be installed. The result is
/// what `cargo fmt` leaves alone.
pub fn format(code: &str) -> std::io::Result<String> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let mut rustfmt = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut input = rustfmt.stdin.take().expect("the input was piped");
    let code = code.to_string();
    let writer = std::thread::spawn(move || input.write_all(code.as_bytes()));
    let output = rustfmt.wait_with_output()?;
    writer.join().expect("writing does not panic")?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "rustfmt failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    String::from_utf8(output.stdout).map_err(std::io::Error::other)
}
