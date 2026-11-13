//! A checked schema: every name resolved, and every type, field, variant,
//! error and method defined once in its scope.
//!
//! A schema is one file, or a folder of them. In a folder, `protocol.anyx`
//! holds the `PROTOCOL` header alone, and every other file is a scope named
//! by its path: `files.anyx` is the scope `files`, and `files/export.anyx`
//! the scope `files/export`. A method in a scope is called by its full name,
//! `files/export/midi _`. A type or an error is private to its scope unless
//! it is `PUBLIC`, and another scope refers to it by its full path,
//! `common/diagnostic`.

use anyx::{NamePart, Node, NodeKind, Span, line_column};

use crate::check::Checker;
use crate::grammar as ast;

/// The file of a schema's folder that holds its `PROTOCOL` header, and the
/// name a schema of one file is read under.
pub const ROOT_FILE: &str = "protocol.anyx";

const EXTENSION: &str = ".anyx";

/// One file of a schema, as given to [`Schema::parse_files`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaFile<'a> {
    /// Relative to the schema's folder, with `/` between folders:
    /// `files/export.anyx`.
    pub path: &'a str,
    pub source: &'a str,
}

/// A schema that passed every check.
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    /// The files as written, which the guide shows as they are: the root
    /// first, then each scope in order of its path.
    pub files: Vec<SourceFile>,
    pub doc: Option<String>,
    pub name: String,
    pub version: u32,
    pub types: Vec<TypeDefinition>,
    pub errors: Vec<ErrorDefinition>,
    pub methods: Vec<Method>,
}

/// A file of a checked schema.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceFile {
    pub path: String,
    /// The scope it declares: empty for the root.
    pub scope: Vec<String>,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeDefinition {
    /// The scope it is declared in: empty for the root.
    pub scope: Vec<String>,
    /// `PUBLIC`: other scopes may use it.
    pub public: bool,
    pub doc: Option<String>,
    /// Lowercase words, such as `file summary`.
    pub name: String,
    pub kind: TypeKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    Record(Vec<Field>),
    /// Variants, each written in its own shape.
    Choice(Vec<Shaped>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub doc: Option<String>,
    pub name: String,
    pub r#type: Type,
    /// `OPTIONAL`: the field may be left out.
    pub optional: bool,
}

/// A call's shape: words with a typed slot between or around them, such as
/// `conflict at revision (revision: INTEGER)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// The words with `_` for each slot: `conflict at revision _`.
    pub text: String,
    pub parameters: Vec<Parameter>,
}

impl Shape {
    /// The words alone, which name it: `conflict at revision`.
    pub fn words(&self) -> String {
        self.text
            .split(' ')
            .filter(|word| *word != "_")
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A choice's variant, or an error: a shape with a doc.
#[derive(Clone, Debug, PartialEq)]
pub struct Shaped {
    pub doc: Option<String>,
    pub shape: Shape,
}

/// An `ERROR`. On the wire it is written in its shape alone, without its
/// scope.
#[derive(Clone, Debug, PartialEq)]
pub struct ErrorDefinition {
    pub scope: Vec<String>,
    pub public: bool,
    pub error: Shaped,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Method {
    /// The scope it is declared in, which its full name starts with.
    pub scope: Vec<String>,
    pub doc: Option<String>,
    pub shape: Shape,
    /// What a call returns when it succeeds, named.
    pub returns: Parameter,
    /// Indices into [`Schema::errors`], with aliases expanded, in the order
    /// written.
    pub errors: Vec<usize>,
}

impl Method {
    /// What a request calls it by: its scope's path and its shape,
    /// `files/export/midi _`, or its shape alone in the root.
    pub fn full_name(&self) -> String {
        scoped(&self.scope, &self.shape.text)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub r#type: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Text,
    /// Raw bytes, such as a file. On the wire, a base64 string.
    Bytes,
    Integer,
    Number,
    Boolean,
    List(Box<Type>),
    /// The index of a type in [`Schema::types`].
    Named(usize),
}

/// `name` behind the path of `scope`: `common/diagnostic`, or `name` alone
/// in the root.
pub fn scoped(scope: &[String], name: &str) -> String {
    scope
        .iter()
        .map(String::as_str)
        .chain([name])
        .collect::<Vec<_>>()
        .join("/")
}

/// The file that declares `scope`: `files/export.anyx`.
pub fn scope_file(scope: &[String]) -> String {
    match scope.is_empty() {
        true => ROOT_FILE.to_string(),
        false => format!("{}{EXTENSION}", scope.join("/")),
    }
}

/// A file with the scope its path declares.
struct ScopedFile<'a> {
    scope: Vec<String>,
    file: SchemaFile<'a>,
}

/// A file read and decoded, for the checker.
pub(crate) struct DecodedFile {
    pub scope: Vec<String>,
    pub document: ast::Document,
}

impl Schema {
    /// Reads and checks a schema of one file.
    pub fn parse(source: &str) -> Result<Schema, Vec<Diagnostic>> {
        Schema::parse_files(&[SchemaFile {
            path: ROOT_FILE,
            source,
        }])
    }

    /// Reads and checks a schema of a folder of files: `protocol.anyx` and
    /// a file per scope.
    pub fn parse_files(files: &[SchemaFile]) -> Result<Schema, Vec<Diagnostic>> {
        let mut scoped_files = files
            .iter()
            .map(|file| file_scope(files, file).map(|scope| ScopedFile { scope, file: *file }))
            .collect::<Result<Vec<_>, _>>()?;
        scoped_files.sort_by(|left, right| left.scope.cmp(&right.scope));
        let mut decoded = Vec::new();
        let mut diagnostics = Vec::new();
        for scoped in &scoped_files {
            match decode_file(&scoped.file) {
                Ok(document) => decoded.push(DecodedFile {
                    scope: scoped.scope.clone(),
                    document,
                }),
                Err(found) => diagnostics.extend(found),
            }
        }
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        let sources = scoped_files
            .iter()
            .map(|scoped| SourceFile {
                path: scoped.file.path.to_string(),
                scope: scoped.scope.clone(),
                source: scoped.file.source.to_string(),
            })
            .collect();
        let mut checker = Checker::default();
        let schema = checker.schema(sources, &decoded);
        let mut diagnostics = checker.diagnostics;
        if diagnostics.is_empty() {
            return Ok(schema);
        }
        diagnostics.sort_by_key(|diagnostic| {
            (
                scoped_files
                    .iter()
                    .position(|scoped| scoped.file.path == diagnostic.path),
                diagnostic.span.start,
                diagnostic.message.clone(),
            )
        });
        diagnostics.dedup();
        Err(diagnostics)
    }

    pub fn type_name(&self, r#type: &Type) -> String {
        match r#type {
            Type::Text => "TEXT".to_string(),
            Type::Bytes => "BYTES".to_string(),
            Type::Integer => "INTEGER".to_string(),
            Type::Number => "NUMBER".to_string(),
            Type::Boolean => "BOOLEAN".to_string(),
            Type::List(item) => format!("LIST: {}", self.type_name(item)),
            Type::Named(index) => {
                let definition = &self.types[*index];
                scoped(&definition.scope, &definition.name)
            }
        }
    }

    /// The method a request calls by `full_name`, such as `files/read _`.
    pub fn method(&self, full_name: &str) -> Option<&Method> {
        self.methods
            .iter()
            .find(|method| method.full_name() == full_name)
    }

    /// The errors `method` may return.
    pub fn errors_of(&self, method: &Method) -> Vec<&Shaped> {
        method
            .errors
            .iter()
            .map(|index| &self.errors[*index].error)
            .collect()
    }

    /// Every scope, each after the scope it is in, from the root: the scopes
    /// of the files, and the folders they are in.
    pub fn scopes(&self) -> Vec<Vec<String>> {
        let mut scopes: Vec<Vec<String>> = self
            .files
            .iter()
            .flat_map(|file| (0..=file.scope.len()).map(|length| file.scope[..length].to_vec()))
            .collect();
        scopes.sort();
        scopes.dedup();
        scopes
    }
}

/// The scope `file` declares, from its path.
fn file_scope(files: &[SchemaFile], file: &SchemaFile) -> Result<Vec<String>, Vec<Diagnostic>> {
    let at_start = |path: &str, message: String| {
        vec![Diagnostic {
            path: path.to_string(),
            span: Span::empty(0),
            message,
        }]
    };
    if files.iter().filter(|other| other.path == file.path).count() > 1 {
        return Err(at_start(
            file.path,
            format!(
                "found `{}` twice among the schema's files; expected each path once",
                file.path
            ),
        ));
    }
    if !files.iter().any(|other| other.path == ROOT_FILE) {
        return Err(at_start(
            ROOT_FILE,
            format!(
                "found no `{ROOT_FILE}` among the schema's files; expected it, with the `PROTOCOL` header"
            ),
        ));
    }
    if file.path == ROOT_FILE {
        return Ok(Vec::new());
    }
    let segments: Option<Vec<String>> = file
        .path
        .strip_suffix(EXTENSION)
        .map(|stem| stem.split('/').map(str::to_string).collect());
    match segments {
        Some(segments) if segments.iter().all(|segment| is_scope_word(segment)) => Ok(segments),
        _ => Err(at_start(
            file.path,
            format!(
                "found the schema file `{}`; expected a path of single lowercase words, each of ASCII letters and digits starting with a letter, ending in `{EXTENSION}`, such as `files/export{EXTENSION}`, since each word names a scope that requests write",
                file.path
            ),
        )),
    }
}

fn is_scope_word(word: &str) -> bool {
    word.starts_with(|c: char| c.is_ascii_lowercase())
        && word
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// Parses `file` and decodes it with the schema language's grammar.
fn decode_file(file: &SchemaFile) -> Result<ast::Document, Vec<Diagnostic>> {
    let found = |span: Span, message: String| Diagnostic {
        path: file.path.to_string(),
        span,
        message,
    };
    let parsed = anyx::parse(file.source);
    if !parsed.errors.is_empty() {
        return Err(parsed
            .errors
            .into_iter()
            .map(|error| found(error.span, error.message))
            .collect());
    }
    let nodes = parsed.syntax.nodes();
    ast::decode_document(&nodes).map_err(|error| {
        if let Some(shape) = slashed_method(&nodes, error.span) {
            let name = &file.source[shape.start..shape.end];
            return vec![found(
                shape,
                format!(
                    "found the method `{name}`, with `/` in its name; expected words alone, since a method's scope is its file: `METHOD read` in `files.anyx` is called as `files/read`"
                ),
            )];
        }
        let message = match unnamed_return(&nodes, error.span) {
            true => "found a `RETURNS` without a name; expected `RETURNS (name: TYPE)`, such as `RETURNS (new revision: INTEGER)`, since the name says what the value is".to_string(),
            false => error.message(),
        };
        vec![found(error.span, message)]
    })
}

/// The argument after `word` in a top-level call that has it, if `span` is
/// inside it, such as the name of a `METHOD`.
fn after_word<'a>(nodes: &'a [Node], span: Span, word: &str) -> Option<&'a Node> {
    nodes.iter().find_map(|node| {
        let NodeKind::Call(call) = &node.kind else {
            return None;
        };
        let mut slot = 0;
        call.name.iter().find_map(|part| match part {
            NamePart::Slot => {
                slot += 1;
                None
            }
            NamePart::Word { text, .. } if text == word => call
                .arguments
                .get(slot)
                .filter(|after| after.span.start <= span.start && span.end <= after.span.end),
            NamePart::Word { .. } => None,
        })
    })
}

/// Whether `span` is inside what a `METHOD` returns, written without a name:
/// `RETURNS (TEXT)` rather than `RETURNS (text: TEXT)`.
fn unnamed_return(nodes: &[Node], span: Span) -> bool {
    after_word(nodes, span, "RETURNS").is_some_and(|returned| {
        !matches!(&returned.kind, NodeKind::Call(call)
            if call.name.iter().any(|part| matches!(part, NamePart::Word { text, .. } if !anyx::is_keyword(text)))
                && call.name.contains(&NamePart::Slot))
    })
}

/// Where the name of the `METHOD` that `span` is in is, if it has a `/`, as
/// in `METHOD files/read (path: TEXT)`.
fn slashed_method(nodes: &[Node], span: Span) -> Option<Span> {
    let shape = after_word(nodes, span, "METHOD")?;
    let NodeKind::Call(call) = &shape.kind else {
        return None;
    };
    call.name
        .iter()
        .any(|part| matches!(part, NamePart::Word { text, .. } if text == "/"))
        .then_some(shape.span)
}

/// Something wrong in a schema, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    /// The file it is in, as given to [`Schema::parse_files`], or
    /// [`ROOT_FILE`] for a schema of one file.
    pub path: String,
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    /// `file:line:column: message`, then the line with the span underlined.
    /// `source` is the file at [`Diagnostic::path`].
    pub fn render(&self, source: &str, file: &str) -> String {
        let position = line_column(source, self.span.start);
        let line = source.lines().nth(position.line - 1).unwrap_or("");
        let start = self.span.start.min(source.len());
        let width = source[start..self.span.end.clamp(start, source.len())]
            .lines()
            .next()
            .map_or(1, |text| text.chars().count().max(1));
        format!(
            "{file}:{}:{}: {}\n  {line}\n  {}{}",
            position.line,
            position.column,
            self.message,
            " ".repeat(position.column - 1),
            "^".repeat(width)
        )
    }
}
