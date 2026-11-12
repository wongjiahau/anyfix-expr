//! What the generator reports about a grammar: errors and warnings.

use anyx::{LineColumn, Span, line_column};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The grammar can't be generated from.
    Error,
    /// The grammar works, but probably not as meant.
    Warning,
}

/// One problem in a grammar: what was found, what was expected, and the
/// fix, in one line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: message.into(),
        }
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            span,
            message: message.into(),
        }
    }

    /// Where the problem starts in `source`, as a line and column.
    pub fn position(&self, source: &str) -> LineColumn {
        line_column(source, self.span.start)
    }

    /// `file:line:column: error: message`, then the source line with `^`
    /// under the span.
    pub fn render(&self, source: &str, file: &str) -> String {
        let at = self.position(source);
        let line = source.lines().nth(at.line - 1).unwrap_or("");
        let width = source
            .get(self.span.start..self.span.end)
            .map_or(1, |text| text.lines().next().unwrap_or("").chars().count())
            .max(1);
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        format!(
            "{file}:{at}: {severity}: {}\n  {line}\n  {}{}",
            self.message,
            " ".repeat(at.column - 1),
            "^".repeat(width)
        )
    }
}

impl From<anyx::Error> for Diagnostic {
    fn from(error: anyx::Error) -> Self {
        Diagnostic::error(error.span, error.message)
    }
}

impl From<crate::grammar::DecodeError> for Diagnostic {
    fn from(error: crate::grammar::DecodeError) -> Self {
        Diagnostic::error(error.span, error.message())
    }
}
