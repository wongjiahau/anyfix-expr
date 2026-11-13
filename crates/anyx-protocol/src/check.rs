//! Checks a decoded schema: resolves every name in the scope it is written
//! in, keeps private types and errors to their scope, and reports what is
//! defined twice.

use anyx::{NamePart, Span, StringLiteral};

use crate::grammar as ast;
use crate::schema::{
    DecodedFile, Diagnostic, ErrorDefinition, Field, Method, Parameter, ROOT_FILE, Schema, Shape,
    Shaped, SourceFile, Type, TypeDefinition, TypeKind, scope_file, scoped,
};

/// A type, error or alias, where it is declared.
struct Declared {
    scope: Vec<String>,
    name: String,
    span: Span,
    /// The index of its file.
    file: usize,
    public: bool,
}

/// What an `ERRORS` list can refer to.
enum ErrorName<'a> {
    /// An index into the errors.
    Error(usize),
    /// An alias and its members, unresolved.
    Alias(&'a [ast::Reference]),
}

/// An error or an alias. They share one namespace per scope, since an
/// `ERRORS` list refers to either by name.
struct ErrorEntry<'a> {
    declared: Declared,
    kind: ErrorName<'a>,
}

/// A reference read as a path.
struct Path {
    scope: Vec<String>,
    name: String,
    /// Written as a full path, `common/diagnostic`, rather than a name.
    full: bool,
}

/// What a `RECORD`, `CHOICE`, `ERROR` or `ERRORS` declares, public or not.
struct Declaration<'b, T> {
    doc: &'b Option<ast::Spanned<StringLiteral>>,
    public: bool,
    body: &'b T,
}

fn record(record: &ast::Record) -> Declaration<'_, ast::RecordBody> {
    match record {
        ast::Record::Public { doc, body, .. } => Declaration {
            doc,
            public: true,
            body,
        },
        ast::Record::Private { doc, body, .. } => Declaration {
            doc,
            public: false,
            body,
        },
    }
}

fn choice(choice: &ast::Choice) -> Declaration<'_, ast::ChoiceBody> {
    match choice {
        ast::Choice::Public { doc, body, .. } => Declaration {
            doc,
            public: true,
            body,
        },
        ast::Choice::Private { doc, body, .. } => Declaration {
            doc,
            public: false,
            body,
        },
    }
}

fn error(error: &ast::Error) -> Declaration<'_, ast::Shape> {
    match error {
        ast::Error::Public { doc, shape, .. } => Declaration {
            doc,
            public: true,
            body: shape,
        },
        ast::Error::Private { doc, shape, .. } => Declaration {
            doc,
            public: false,
            body: shape,
        },
    }
}

fn errors(errors: &ast::Errors) -> Declaration<'_, ast::ErrorsBody> {
    match errors {
        ast::Errors::Public { doc, body, .. } => Declaration {
            doc,
            public: true,
            body,
        },
        ast::Errors::Private { doc, body, .. } => Declaration {
            doc,
            public: false,
            body,
        },
    }
}

fn doc(doc: &Option<ast::Spanned<StringLiteral>>) -> Option<String> {
    doc.as_ref().map(|doc| doc.value.value.clone())
}

fn spanned_words(name: &ast::CallName) -> ast::Spanned<String> {
    ast::Spanned {
        value: name.words().collect::<Vec<_>>().join(" "),
        span: name.span,
    }
}

#[derive(Default)]
pub(crate) struct Checker<'a> {
    paths: Vec<String>,
    sources: Vec<String>,
    /// The file being checked, which diagnostics are in.
    file: usize,
    /// Its scope, which names are resolved in.
    scope: Vec<String>,
    /// Each type, in order.
    types: Vec<Declared>,
    error_names: Vec<ErrorEntry<'a>>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl<'a> Checker<'a> {
    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            path: self.paths[self.file].clone(),
            span,
            message: message.into(),
        });
    }

    /// Checks within `file` and its `scope`, then goes back.
    fn within<T>(
        &mut self,
        file: usize,
        scope: &[String],
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let file = std::mem::replace(&mut self.file, file);
        let scope = std::mem::replace(&mut self.scope, scope.to_vec());
        let checked = check(self);
        self.file = file;
        self.scope = scope;
        checked
    }

    /// The source at `span` in the file being checked.
    fn written(&self, span: Span) -> &str {
        &self.sources[self.file][span.start..span.end]
    }

    /// The scope being checked, in a message: `this schema` for one of a
    /// single file, else `the scope `files``.
    fn this_scope(&self) -> String {
        match self.scope.is_empty() {
            true => "this schema".to_string(),
            false => format!("the scope `{}`", self.scope.join("/")),
        }
    }

    pub(crate) fn schema(&mut self, files: Vec<SourceFile>, decoded: &'a [DecodedFile]) -> Schema {
        self.paths = files.iter().map(|file| file.path.clone()).collect();
        self.sources = files.iter().map(|file| file.source.clone()).collect();
        for (index, file) in decoded.iter().enumerate() {
            self.within(index, &file.scope, |checker| {
                checker.header(file, decoded.len() > 1)
            });
        }
        let mut error_count = 0;
        for (index, file) in decoded.iter().enumerate() {
            self.file = index;
            self.scope = file.scope.clone();
            for definition in &file.document.definition {
                match definition {
                    ast::Definition::Record(found) => {
                        let found = record(found);
                        self.declare_type(&found.body.name, found.public);
                    }
                    ast::Definition::Choice(found) => {
                        let found = choice(found);
                        self.declare_type(&found.body.name, found.public);
                    }
                    ast::Definition::Error(found) => {
                        let found = error(found);
                        let words = spanned_words(&found.body.name);
                        self.declare_error(&words, found.public, ErrorName::Error(error_count));
                        error_count += 1;
                    }
                    ast::Definition::Errors(found) => {
                        let found = errors(found);
                        self.check_words(&found.body.name, "an error alias");
                        self.declare_error(
                            &found.body.name,
                            found.public,
                            ErrorName::Alias(&found.body.members.value),
                        );
                    }
                    ast::Definition::Method(_) => {}
                }
            }
        }
        let mut types = Vec::new();
        let mut errors_found = Vec::new();
        let mut methods: Vec<Method> = Vec::new();
        for (index, file) in decoded.iter().enumerate() {
            self.file = index;
            self.scope = file.scope.clone();
            for definition in &file.document.definition {
                match definition {
                    ast::Definition::Record(found) => {
                        let found = record(found);
                        types.push(TypeDefinition {
                            scope: file.scope.clone(),
                            public: found.public,
                            doc: doc(found.doc),
                            name: found.body.name.value.clone(),
                            kind: TypeKind::Record(self.fields(&found.body.fields.value)),
                        });
                    }
                    ast::Definition::Choice(found) => {
                        let found = choice(found);
                        let variants = self.variants(&found.body.variants.value);
                        types.push(TypeDefinition {
                            scope: file.scope.clone(),
                            public: found.public,
                            doc: doc(found.doc),
                            name: found.body.name.value.clone(),
                            kind: TypeKind::Choice(variants),
                        });
                    }
                    ast::Definition::Error(found) => {
                        let found = error(found);
                        let shape = self.shape(&found.body.name, &found.body.arguments, "an error");
                        errors_found.push(ErrorDefinition {
                            scope: file.scope.clone(),
                            public: found.public,
                            error: Shaped {
                                doc: doc(found.doc),
                                shape,
                            },
                        });
                    }
                    ast::Definition::Errors(found) => {
                        // Reports unknown members and cycles once, here.
                        let name = &errors(found).body.name.value;
                        if let Some(entry) = self.error_names.iter().position(|entry| {
                            entry.declared.scope == file.scope && entry.declared.name == *name
                        }) {
                            self.expand(entry, &mut Vec::new());
                        }
                    }
                    ast::Definition::Method(found) => {
                        let method = self.method(found);
                        if methods.iter().any(|other| {
                            other.scope == method.scope && other.shape.text == method.shape.text
                        }) {
                            let (ast::Method::WithErrors { shape, .. }
                            | ast::Method::WithoutErrors { shape, .. }) = found;
                            self.error(
                                shape.name.span,
                                format!(
                                    "found a second method `{}`; expected each method shape once in a scope; rename or remove one of them",
                                    method.shape.text
                                ),
                            );
                        }
                        methods.push(method);
                    }
                }
            }
        }
        let header = decoded
            .first()
            .and_then(|root| root.document.header.as_ref())
            .filter(|_| decoded[0].scope.is_empty());
        let name = header.map(|header| {
            self.within(0, &[], |checker| {
                let name = spanned_words(&header.name.name);
                checker.check_words(&name, "a protocol");
                name.value
            })
        });
        let schema = Schema {
            files,
            doc: header.and_then(|header| doc(&header.doc)),
            name: name.unwrap_or_default(),
            version: header.map_or(0, |header| header.version.value),
            types,
            errors: errors_found,
            methods,
        };
        self.check_recursion(&schema);
        schema
    }

    /// The root has the `PROTOCOL` header, alone when there are scope files;
    /// a scope file has none.
    fn header(&mut self, file: &DecodedFile, several: bool) {
        let path = self.paths[self.file].clone();
        match (&file.document.header, file.scope.is_empty()) {
            (Some(_), true) | (None, false) => {}
            (None, true) => self.error(
                Span::empty(0),
                "found no `PROTOCOL` header; expected the schema to start with one, such as `PROTOCOL notes VERSION 1`",
            ),
            (Some(header), false) => self.error(
                header.span,
                format!(
                    "found a `PROTOCOL` header in `{path}`; expected it only in `{ROOT_FILE}`, since this file is the scope `{}`; remove it",
                    file.scope.join("/")
                ),
            ),
        }
        if let Some(first) = file.document.definition.first()
            && several
            && file.scope.is_empty()
        {
            self.error(
                first.span(),
                format!(
                    "found a definition in `{ROOT_FILE}`; expected only the `PROTOCOL` header there, since the schema has a file per scope; move it into a scope's file, such as `common.anyx`"
                ),
            );
        }
    }

    fn declare_type(&mut self, name: &ast::Spanned<String>, public: bool) {
        self.check_words(name, "a type");
        if self
            .types
            .iter()
            .any(|known| known.scope == self.scope && known.name == name.value)
        {
            self.error(
                name.span,
                format!(
                    "found a second type named `{}`; expected each type to be defined once in a scope; rename or remove one of them",
                    name.value
                ),
            );
        }
        let declared = self.declared(name, public);
        self.types.push(declared);
    }

    fn declared(&self, name: &ast::Spanned<String>, public: bool) -> Declared {
        Declared {
            scope: self.scope.clone(),
            name: name.value.clone(),
            span: name.span,
            file: self.file,
            public,
        }
    }

    fn declare_error(&mut self, words: &ast::Spanned<String>, public: bool, kind: ErrorName<'a>) {
        if self
            .error_names
            .iter()
            .any(|known| known.declared.scope == self.scope && known.declared.name == words.value)
        {
            self.error(
                words.span,
                format!(
                    "found a second error or error alias named `{}`; expected each name once in a scope, since an `ERRORS` list refers to them by name; rename one of them",
                    words.value
                ),
            );
        }
        let declared = self.declared(words, public);
        self.error_names.push(ErrorEntry { declared, kind });
    }

    /// `reference` as a path: a name in the scope being checked, or a full
    /// path. `None` after reporting it when it is neither.
    fn path(&mut self, reference: &ast::Reference) -> Option<Path> {
        let parts = &reference.name.parts;
        if reference.arguments.is_empty() && !parts.contains(&NamePart::Slot) {
            return Some(Path {
                scope: self.scope.clone(),
                name: spanned_words(&reference.name).value,
                full: false,
            });
        }
        let slashes_between_slots = parts.len() >= 3
            && parts.len() % 2 == 1
            && parts.iter().enumerate().all(|(index, part)| match part {
                NamePart::Slot => index % 2 == 0,
                NamePart::Word { text, .. } => index % 2 == 1 && text == "/",
            });
        let path = reference.arguments.split_last().filter(|(_, scope)| {
            scope
                .iter()
                .all(|segment| matches!(segment.name.parts.as_slice(), [NamePart::Word { .. }]))
        });
        match path {
            Some((name, scope)) if slashes_between_slots => Some(Path {
                scope: scope
                    .iter()
                    .map(|segment| segment.name.text.clone())
                    .collect(),
                name: spanned_words(&name.name).value,
                full: true,
            }),
            _ => {
                let written = self.written(reference.span).to_string();
                self.error(
                    reference.span,
                    format!(
                        "found `{written}`; expected a name, such as `diagnostic`, or the full path of a `PUBLIC` one in another scope, such as `common/diagnostic`"
                    ),
                );
                None
            }
        }
    }

    /// Reports `found` when `path` reaches it from another scope and it is
    /// private.
    fn check_access(&mut self, span: Span, path: &Path, public: bool, what: &str) {
        if public || path.scope == self.scope {
            return;
        }
        self.error(
            span,
            format!(
                "found `{}`, which is private to the scope `{}`; expected a `PUBLIC` {what}, or one of {}; write `PUBLIC` before its definition in `{}` to share it",
                scoped(&path.scope, &path.name),
                path.scope.join("/"),
                self.this_scope(),
                scope_file(&path.scope)
            ),
        );
    }

    /// The full paths of the public declarations named `name` in other
    /// scopes, as a hint after a name that isn't in this scope.
    fn elsewhere<'d>(&self, declared: impl Iterator<Item = &'d Declared>, name: &str) -> String {
        let paths: Vec<String> = declared
            .filter(|known| known.public && known.name == name && known.scope != self.scope)
            .map(|known| format!("`{}`", scoped(&known.scope, &known.name)))
            .collect();
        match paths.is_empty() {
            true => String::new(),
            false => format!(
                "; another scope has {}, so write its full path",
                paths.join(", ")
            ),
        }
    }

    fn resolve_type(&mut self, reference: &ast::Reference) -> Option<usize> {
        let path = self.path(reference)?;
        let found = self
            .types
            .iter()
            .position(|known| known.scope == path.scope && known.name == path.name);
        match found {
            Some(index) => {
                let public = self.types[index].public;
                self.check_access(reference.span, &path, public, "type");
                Some(index)
            }
            None => {
                let message = match path.full {
                    true => format!(
                        "found `{}`, which names no type; expected the full path of a `PUBLIC` `RECORD` or `CHOICE`, such as `common/diagnostic` for `PUBLIC RECORD diagnostic` in `common.anyx`",
                        scoped(&path.scope, &path.name)
                    ),
                    false => format!(
                        "found `{text}`, which is not a type of {scope}; expected `TEXT`, `BYTES`, `INTEGER`, `NUMBER`, `BOOLEAN`, `LIST: type`, or a `RECORD` or `CHOICE` defined here; define `{text}`, or fix the name{hint}",
                        text = path.name,
                        scope = self.this_scope(),
                        hint = self.elsewhere(self.types.iter(), &path.name),
                    ),
                };
                self.error(reference.span, message);
                None
            }
        }
    }

    /// The error or alias `reference` names, as an index into the error
    /// names.
    fn resolve_error(&mut self, reference: &ast::Reference) -> Option<usize> {
        let path = self.path(reference)?;
        let found = self.error_names.iter().position(|known| {
            known.declared.scope == path.scope && known.declared.name == path.name
        });
        match found {
            Some(index) => {
                let public = self.error_names[index].declared.public;
                self.check_access(reference.span, &path, public, "error or alias");
                Some(index)
            }
            None => {
                let message = match path.full {
                    true => format!(
                        "found `{}`, which names no error; expected the full path of a `PUBLIC` `ERROR` or `ERRORS` alias, such as `files/missing` for `PUBLIC ERROR missing` in `files.anyx`",
                        scoped(&path.scope, &path.name)
                    ),
                    false => format!(
                        "found `{}`, which is not an error of {}; expected the words of an `ERROR`, such as `conflict at revision` for `ERROR conflict at revision (revision: INTEGER)`, or an `ERRORS` alias{}",
                        path.name,
                        self.this_scope(),
                        self.elsewhere(
                            self.error_names.iter().map(|entry| &entry.declared),
                            &path.name
                        ),
                    ),
                };
                self.error(reference.span, message);
                None
            }
        }
    }

    /// The errors that the error or alias at `entry` stands for, in order,
    /// or `None` when it can't be resolved. `visiting` holds the aliases
    /// being expanded, to catch one that includes itself.
    fn expand(&mut self, entry: usize, visiting: &mut Vec<usize>) -> Option<Vec<usize>> {
        let members: &'a [ast::Reference] = match &self.error_names[entry].kind {
            ErrorName::Error(index) => return Some(vec![*index]),
            ErrorName::Alias(members) => members,
        };
        let declared = &self.error_names[entry].declared;
        let (file, scope, span, name) = (
            declared.file,
            declared.scope.clone(),
            declared.span,
            declared.name.clone(),
        );
        self.within(file, &scope, |checker| {
            if visiting.contains(&entry) {
                checker.error(
                    span,
                    format!(
                        "found the error alias `{name}` including itself; expected an alias to name errors, or aliases that don't lead back to it"
                    ),
                );
                return None;
            }
            visiting.push(entry);
            let mut expanded = Vec::new();
            for member in members {
                let Some(found) = checker.resolve_error(member) else {
                    continue;
                };
                expanded.extend(checker.expand(found, visiting)?);
            }
            visiting.pop();
            Some(expanded)
        })
    }

    /// The declaration of the error at `index` in the errors.
    fn error_declared(&self, index: usize) -> Option<&Declared> {
        self.error_names
            .iter()
            .find(|entry| matches!(entry.kind, ErrorName::Error(known) if known == index))
            .map(|entry| &entry.declared)
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
        let references: Vec<&ast::Reference> = match list {
            None => Vec::new(),
            Some(ast::ErrorList::One(reference)) => vec![reference],
            Some(ast::ErrorList::Several(references)) => references.value.iter().collect(),
        };
        let mut errors: Vec<usize> = Vec::new();
        for reference in references {
            let Some(entry) = self.resolve_error(reference) else {
                continue;
            };
            for index in self.expand(entry, &mut Vec::new()).into_iter().flatten() {
                let declared = self
                    .error_declared(index)
                    .map(|declared| (declared.scope.clone(), declared.name.clone()));
                let same_words = errors.iter().find_map(|known| {
                    self.error_declared(*known)
                        .filter(|other| {
                            declared
                                .as_ref()
                                .is_some_and(|(_, name)| other.name == *name)
                        })
                        .map(|other| scoped(&other.scope, &other.name))
                });
                if errors.contains(&index) {
                    let words = declared.map_or(String::new(), |(_, name)| name);
                    self.error(
                        reference.span,
                        format!(
                            "found the error `{words}` listed twice for this method, counting the errors in its aliases; expected each error once; remove one"
                        ),
                    );
                } else if let Some(other) = same_words {
                    let this =
                        declared.map_or(String::new(), |(scope, name)| scoped(&scope, &name));
                    self.error(
                        reference.span,
                        format!(
                            "found the errors `{other}` and `{this}` both listed for this method; expected errors of distinct words, since a response writes an error by its shape alone, without its scope; rename one of them"
                        ),
                    );
                } else {
                    errors.push(index);
                }
            }
        }
        Method {
            scope: self.scope.clone(),
            doc: doc(doc_string),
            shape: checked_shape,
            returns: self.parameter(returns),
            errors,
        }
    }

    fn r#type(&mut self, r#type: &ast::Type) -> Type {
        match r#type {
            ast::Type::Text(_) => Type::Text,
            ast::Type::Bytes(_) => Type::Bytes,
            ast::Type::Integer(_) => Type::Integer,
            ast::Type::Number(_) => Type::Number,
            ast::Type::Boolean(_) => Type::Boolean,
            ast::Type::List { item, .. } => Type::List(Box::new(self.r#type(item))),
            ast::Type::Named(reference) => {
                self.resolve_type(reference).map_or(Type::Text, Type::Named)
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
                    let declared = &self.types[start];
                    let (file, span, name) = (declared.file, declared.span, declared.name.clone());
                    self.file = file;
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
