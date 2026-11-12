//! A checked schema: every name resolved, and every type, field, variant,
//! error and method defined once.

use anyx::{NamePart, Node, NodeKind, Span, line_column};

use crate::grammar as ast;

/// A schema that passed every check.
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    /// The schema as written, which the guide shows as it is.
    pub source: String,
    pub doc: Option<String>,
    pub name: String,
    pub version: u32,
    pub types: Vec<TypeDefinition>,
    pub errors: Vec<Shaped>,
    pub methods: Vec<Method>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeDefinition {
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

#[derive(Clone, Debug, PartialEq)]
pub struct Method {
    pub doc: Option<String>,
    pub shape: Shape,
    /// What a call returns when it succeeds, named.
    pub returns: Parameter,
    /// Indices into [`Schema::errors`], with aliases expanded, in the order
    /// written.
    pub errors: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub r#type: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Text,
    Integer,
    Number,
    Boolean,
    List(Box<Type>),
    /// The index of a type in [`Schema::types`].
    Named(usize),
}

impl Schema {
    /// Reads and checks the schema in `source`.
    pub fn parse(source: &str) -> Result<Schema, Vec<Diagnostic>> {
        let parsed = anyx::parse(source);
        if !parsed.errors.is_empty() {
            return Err(parsed
                .errors
                .into_iter()
                .map(|error| Diagnostic {
                    span: error.span,
                    message: error.message,
                })
                .collect());
        }
        let nodes = parsed.syntax.nodes();
        let document = ast::decode_document(&nodes).map_err(|error| {
            let message = match unnamed_return(&nodes, error.span) {
                true => "found a `RETURNS` without a name; expected `RETURNS (name: TYPE)`, such as `RETURNS (new revision: INTEGER)`, since the name says what the value is".to_string(),
                false => error.message(),
            };
            vec![Diagnostic {
                span: error.span,
                message,
            }]
        })?;
        let mut checker = Checker::default();
        let schema = checker.schema(source, &document);
        let mut diagnostics = checker.diagnostics;
        if diagnostics.is_empty() {
            return Ok(schema);
        }
        diagnostics.sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.message.clone()));
        diagnostics.dedup();
        Err(diagnostics)
    }

    pub fn type_name(&self, r#type: &Type) -> String {
        match r#type {
            Type::Text => "TEXT".to_string(),
            Type::Integer => "INTEGER".to_string(),
            Type::Number => "NUMBER".to_string(),
            Type::Boolean => "BOOLEAN".to_string(),
            Type::List(item) => format!("LIST: {}", self.type_name(item)),
            Type::Named(index) => self.types[*index].name.clone(),
        }
    }

    /// The method called in `shape`, such as `read _`.
    pub fn method(&self, shape: &str) -> Option<&Method> {
        self.methods
            .iter()
            .find(|method| method.shape.text == shape)
    }

    /// The errors `method` may return.
    pub fn errors_of(&self, method: &Method) -> Vec<&Shaped> {
        method
            .errors
            .iter()
            .map(|index| &self.errors[*index])
            .collect()
    }
}

/// Whether `span` is inside what a `METHOD` returns, written without a name:
/// `RETURNS (TEXT)` rather than `RETURNS (text: TEXT)`.
fn unnamed_return(nodes: &[Node], span: Span) -> bool {
    nodes.iter().any(|node| {
        let NodeKind::Call(call) = &node.kind else {
            return false;
        };
        let mut slot = 0;
        call.name.iter().any(|part| match part {
            NamePart::Slot => {
                slot += 1;
                false
            }
            NamePart::Word { text, .. } => {
                text == "RETURNS"
                    && call.arguments.get(slot).is_some_and(|returned| {
                        let named = matches!(&returned.kind, NodeKind::Call(call)
                            if call.name.iter().any(|part| matches!(part, NamePart::Word { text, .. } if !anyx::is_keyword(text)))
                                && call.name.contains(&NamePart::Slot));
                        !named && returned.span.start <= span.start && span.end <= returned.span.end
                    })
            }
        })
    })
}

/// Something wrong in a schema, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    /// `file:line:column: message`, then the line with the span underlined.
    pub fn render(&self, source: &str, file: &str) -> String {
        let position = line_column(source, self.span.start);
        let line = source.lines().nth(position.line - 1).unwrap_or("");
        let width = source[self.span.start..self.span.end.min(source.len())]
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

/// A name an `ERRORS` list can refer to.
enum ErrorName<'a> {
    /// An index into the errors.
    Error(usize),
    /// An alias and its members, unresolved.
    Alias(&'a [ast::TypeName]),
}

#[derive(Default)]
struct Checker<'a> {
    /// Each type's name and where it is defined, in order.
    types: Vec<(String, Span)>,
    /// Errors and aliases by name. They share one namespace, since an
    /// `ERRORS` list refers to either by name.
    error_names: Vec<(String, Span, ErrorName<'a>)>,
    diagnostics: Vec<Diagnostic>,
}

fn doc(doc: &Option<ast::Spanned<anyx::StringLiteral>>) -> Option<String> {
    doc.as_ref().map(|doc| doc.value.value.clone())
}

fn spanned_words(name: &ast::CallName) -> ast::Spanned<String> {
    ast::Spanned {
        value: name.words().collect::<Vec<_>>().join(" "),
        span: name.span,
    }
}

impl<'a> Checker<'a> {
    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            span,
            message: message.into(),
        });
    }

    fn schema(&mut self, source: &str, document: &'a ast::Document) -> Schema {
        let mut error_count = 0;
        for definition in &document.definition {
            match definition {
                ast::Definition::Record(record) => self.type_name(&record.body.name),
                ast::Definition::Choice(choice) => self.type_name(&choice.body.name),
                ast::Definition::Error(error) => {
                    let words = spanned_words(&error.shape.name);
                    self.error_name(&words, ErrorName::Error(error_count));
                    error_count += 1;
                }
                ast::Definition::Errors(errors) => {
                    self.check_words(&errors.body.name, "an error alias");
                    self.error_name(
                        &errors.body.name,
                        ErrorName::Alias(&errors.body.members.value),
                    );
                }
                ast::Definition::Method(_) => {}
            }
        }
        let mut types = Vec::new();
        let mut errors = Vec::new();
        let mut methods: Vec<Method> = Vec::new();
        for definition in &document.definition {
            match definition {
                ast::Definition::Record(record) => types.push(TypeDefinition {
                    doc: doc(&record.doc),
                    name: record.body.name.value.clone(),
                    kind: TypeKind::Record(self.fields(&record.body.fields.value)),
                }),
                ast::Definition::Choice(choice) => {
                    let variants = self.variants(&choice.body.variants.value);
                    types.push(TypeDefinition {
                        doc: doc(&choice.doc),
                        name: choice.body.name.value.clone(),
                        kind: TypeKind::Choice(variants),
                    });
                }
                ast::Definition::Error(error) => {
                    let shape = self.shape(&error.shape.name, &error.shape.arguments, "an error");
                    errors.push(Shaped {
                        doc: doc(&error.doc),
                        shape,
                    });
                }
                ast::Definition::Errors(errors) => {
                    // Reports unknown members and cycles once, here.
                    self.expand(&errors.body.name.value, &mut Vec::new());
                }
                ast::Definition::Method(method) => {
                    let method = self.method(method);
                    if methods
                        .iter()
                        .any(|other| other.shape.text == method.shape.text)
                    {
                        let span = match definition {
                            ast::Definition::Method(
                                ast::Method::WithErrors { shape, .. }
                                | ast::Method::WithoutErrors { shape, .. },
                            ) => shape.name.span,
                            _ => definition.span(),
                        };
                        self.error(
                            span,
                            format!(
                                "found a second method `{}`; expected each method shape once; rename or remove one of them",
                                method.shape.text
                            ),
                        );
                    }
                    methods.push(method);
                }
            }
        }
        let protocol = spanned_words(&document.protocol.name.name);
        self.check_words(&protocol, "a protocol");
        let schema = Schema {
            source: source.to_string(),
            doc: doc(&document.protocol.doc),
            name: protocol.value,
            version: document.protocol.version.value,
            types,
            errors,
            methods,
        };
        self.check_recursion(&schema);
        schema
    }

    fn type_name(&mut self, name: &ast::Spanned<String>) {
        self.check_words(name, "a type");
        if self.types.iter().any(|(known, _)| *known == name.value) {
            self.error(
                name.span,
                format!(
                    "found a second type named `{}`; expected each type to be defined once; rename or remove one of them",
                    name.value
                ),
            );
        }
        self.types.push((name.value.clone(), name.span));
    }

    fn error_name(&mut self, words: &ast::Spanned<String>, name: ErrorName<'a>) {
        if self
            .error_names
            .iter()
            .any(|(known, ..)| *known == words.value)
        {
            self.error(
                words.span,
                format!(
                    "found a second error or error alias named `{}`; expected each name once, since an `ERRORS` list refers to them by name; rename one of them",
                    words.value
                ),
            );
        }
        self.error_names
            .push((words.value.clone(), words.span, name));
    }

    /// The errors that the error or alias `name` stands for, in order, or
    /// `None` when it can't be resolved. `visiting` holds the aliases being
    /// expanded, to catch one that includes itself.
    fn expand(&mut self, name: &str, visiting: &mut Vec<String>) -> Option<Vec<usize>> {
        let (_, span, kind) = self.error_names.iter().find(|(known, ..)| known == name)?;
        let span = *span;
        let members = match kind {
            ErrorName::Error(index) => return Some(vec![*index]),
            ErrorName::Alias(members) => *members,
        };
        if visiting.iter().any(|alias| alias == name) {
            self.error(
                span,
                format!(
                    "found the error alias `{name}` including itself; expected an alias to name errors, or aliases that don't lead back to it"
                ),
            );
            return None;
        }
        visiting.push(name.to_string());
        let mut expanded = Vec::new();
        for member in members {
            let text = &member.name.text;
            if !self.error_names.iter().any(|(known, ..)| known == text) {
                self.unknown_error(member);
                continue;
            }
            expanded.extend(self.expand(text, visiting)?);
        }
        visiting.pop();
        Some(expanded)
    }

    fn unknown_error(&mut self, name: &ast::TypeName) {
        self.error(
            name.span,
            format!(
                "found `{}`, which is not an error of this schema; expected the words of an `ERROR`, such as `conflict at revision` for `ERROR conflict at revision (revision: INTEGER)`, or an `ERRORS` alias",
                name.name.text
            ),
        );
    }

    /// Names become Rust and TypeScript identifiers, so each word is
    /// lowercase letters and digits.
    fn check_words(&mut self, name: &ast::Spanned<String>, what: &str) {
        let fits = name.value.split(' ').all(|word| {
            word.starts_with(|c: char| c.is_ascii_lowercase())
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        });
        if !fits {
            self.error(
                name.span,
                format!(
                    "found `{}` as the name of {what}; expected words of lowercase ASCII letters and digits, each starting with a letter, since names become Rust and TypeScript identifiers; rename it",
                    name.value
                ),
            );
        }
    }

    fn fields(&mut self, fields: &[ast::Field]) -> Vec<Field> {
        let mut checked: Vec<Field> = Vec::new();
        for field in fields {
            self.check_words(&field.name, "a field");
            if checked.iter().any(|known| known.name == field.name.value) {
                self.error(
                    field.name.span,
                    format!(
                        "found a second field named `{}`; expected each field once; rename or remove one of them",
                        field.name.value
                    ),
                );
            }
            let (r#type, optional) = match &field.r#type {
                ast::FieldType::Optional { r#type, .. } => (r#type, true),
                ast::FieldType::Type(r#type) => (r#type, false),
            };
            checked.push(Field {
                doc: doc(&field.doc),
                name: field.name.value.clone(),
                r#type: self.r#type(r#type),
                optional,
            });
        }
        checked
    }

    fn variants(&mut self, variants: &[ast::Variant]) -> Vec<Shaped> {
        let mut checked: Vec<Shaped> = Vec::new();
        for variant in variants {
            let mut slots = variant.arguments.iter().peekable();
            // A doc fills the call's first slot, before the first word.
            let doc = match (variant.name.parts.first(), slots.peek()) {
                (Some(NamePart::Slot), Some(ast::VariantSlot::Doc(doc))) => {
                    slots.next();
                    Some(doc.value.value.clone())
                }
                _ => None,
            };
            let mut parameters = Vec::new();
            for slot in slots {
                match slot {
                    ast::VariantSlot::Doc(doc) => self.error(
                        doc.span,
                        "found a string in a variant's slot; expected a parameter, as in `(revision: INTEGER)`; a variant's `#` doc goes on the lines before it",
                    ),
                    ast::VariantSlot::Parameter(parameter) => parameters.push(parameter.clone()),
                }
            }
            let mut name = variant.name.clone();
            if doc.is_some() {
                name.parts.remove(0);
                name.text = name.text.trim_start_matches('_').trim_start().to_string();
            }
            let shape = self.shape(&name, &parameters, "a variant");
            if checked
                .iter()
                .any(|known| known.shape.words() == shape.words())
            {
                self.error(
                    name.span,
                    format!(
                        "found a second variant named `{}`; expected each variant's words once; rename or remove one of them",
                        shape.words()
                    ),
                );
            }
            checked.push(Shaped { doc, shape });
        }
        checked
    }

    fn shape(&mut self, name: &ast::CallName, parameters: &[ast::Parameter], what: &str) -> Shape {
        let words = spanned_words(name);
        if words.value.is_empty() {
            self.error(
                name.span,
                format!(
                    "found {what} with no words; expected at least one lowercase word to name it"
                ),
            );
        } else {
            self.check_words(&words, what);
        }
        let mut checked: Vec<Parameter> = Vec::new();
        for parameter in parameters {
            if checked
                .iter()
                .any(|known| known.name == parameter.name.value)
            {
                self.error(
                    parameter.name.span,
                    format!(
                        "found a second parameter named `{}`; expected each parameter once; rename one of them",
                        parameter.name.value
                    ),
                );
            }
            if parameter.name.value == "kind" {
                self.error(
                    parameter.name.span,
                    "found a parameter named `kind`; expected another name, since a variant's JSON form keeps its name in `kind`; rename it",
                );
            }
            checked.push(self.parameter(parameter));
        }
        Shape {
            text: name.text.clone(),
            parameters: checked,
        }
    }

    fn parameter(&mut self, parameter: &ast::Parameter) -> Parameter {
        self.check_words(&parameter.name, "a parameter");
        Parameter {
            name: parameter.name.value.clone(),
            r#type: self.r#type(&parameter.r#type),
        }
    }

    fn method(&mut self, method: &ast::Method) -> Method {
        let (doc_string, shape, returns, list) = match method {
            ast::Method::WithErrors {
                doc,
                shape,
                returns,
                errors,
                ..
            } => (doc, shape, returns, Some(errors)),
            ast::Method::WithoutErrors {
                doc,
                shape,
                returns,
                ..
            } => (doc, shape, returns, None),
        };
        let checked_shape = self.shape(&shape.name, &shape.arguments, "a method");
        let names: Vec<&ast::TypeName> = match list {
            None => Vec::new(),
            Some(ast::ErrorList::One(name)) => vec![name],
            Some(ast::ErrorList::Several(names)) => names.value.iter().collect(),
        };
        let mut errors: Vec<usize> = Vec::new();
        for name in names {
            let text = &name.name.text;
            if !self.error_names.iter().any(|(known, ..)| known == text) {
                self.unknown_error(name);
                continue;
            }
            for index in self.expand(text, &mut Vec::new()).into_iter().flatten() {
                if errors.contains(&index) {
                    let words = self
                        .error_names
                        .iter()
                        .find(|(_, _, kind)| matches!(kind, ErrorName::Error(known) if *known == index))
                        .map_or(String::new(), |(words, ..)| words.clone());
                    self.error(
                        name.span,
                        format!(
                            "found the error `{words}` listed twice for this method, counting the errors in its aliases; expected each error once; remove one"
                        ),
                    );
                } else {
                    errors.push(index);
                }
            }
        }
        Method {
            doc: doc(doc_string),
            shape: checked_shape,
            returns: self.parameter(returns),
            errors,
        }
    }

    fn r#type(&mut self, r#type: &ast::Type) -> Type {
        match r#type {
            ast::Type::Text(_) => Type::Text,
            ast::Type::Integer(_) => Type::Integer,
            ast::Type::Number(_) => Type::Number,
            ast::Type::Boolean(_) => Type::Boolean,
            ast::Type::List { item, .. } => Type::List(Box::new(self.r#type(item))),
            ast::Type::Named(name) => {
                let text = &name.name.text;
                match self.types.iter().position(|(known, _)| known == text) {
                    Some(index) => Type::Named(index),
                    None => {
                        self.error(
                            name.span,
                            format!(
                                "found `{text}`, which is not a type of this schema; expected `TEXT`, `INTEGER`, `NUMBER`, `BOOLEAN`, `LIST: type`, or a `RECORD` or `CHOICE` defined here; define `{text}`, or fix the name"
                            ),
                        );
                        Type::Text
                    }
                }
            }
        }
    }

    /// A type that holds itself, other than through a `LIST`, has no
    /// finite value.
    fn check_recursion(&mut self, schema: &Schema) {
        fn held(r#type: &Type) -> Option<usize> {
            match r#type {
                Type::Named(index) => Some(*index),
                _ => None,
            }
        }
        let edges: Vec<Vec<usize>> = schema
            .types
            .iter()
            .map(|definition| match &definition.kind {
                TypeKind::Record(fields) => fields
                    .iter()
                    .filter_map(|field| held(&field.r#type))
                    .collect(),
                TypeKind::Choice(variants) => variants
                    .iter()
                    .flat_map(|variant| &variant.shape.parameters)
                    .filter_map(|parameter| held(&parameter.r#type))
                    .collect(),
            })
            .collect();
        for start in 0..schema.types.len() {
            let mut seen = vec![false; schema.types.len()];
            let mut pending = edges[start].clone();
            while let Some(next) = pending.pop() {
                if next == start {
                    let (name, span) = self.types[start].clone();
                    self.error(
                        span,
                        format!(
                            "found the type `{name}` holding itself; expected a type that holds itself to do so through a `LIST`, so that its values end; make the field a `LIST`"
                        ),
                    );
                    break;
                }
                if !std::mem::replace(&mut seen[next], true) {
                    pending.extend(&edges[next]);
                }
            }
        }
    }
}
