//! Checks a decoded schema: resolves every name in the scope it is written
//! in, keeps private types and errors to their scope, and reports what is
//! defined twice.

use anyx::{NamePart, Span, StringLiteral};

use crate::error_terms::{self, ErrorTerms, GroupReference, Misplaced};
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

/// An `ERRORS` group: `ERRORS file errors [trashed, missing] + other`.
struct Group<'a> {
    declared: Declared,
    /// Its own errors, as written.
    own: &'a [ast::Error],
    /// Their indices into the errors, once read.
    own_errors: Vec<usize>,
    /// The groups it joins with `+`, unresolved.
    joined: Vec<GroupReference>,
    /// Every error it stands for, once expanded: `None` when it can't be.
    expanded: Option<Option<Vec<usize>>>,
}

/// An error of a method, and where its list names it.
struct Listed {
    index: usize,
    span: Span,
}

/// A reference read as a path.
struct Path {
    scope: Vec<String>,
    name: String,
    /// Written as a full path, `common/diagnostic`, rather than a name.
    full: bool,
}

/// What a `RECORD`, `CHOICE` or `ERRORS` group declares, public or not.
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

fn errors(errors: &ast::Errors) -> Declaration<'_, ast::ErrorList> {
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
    groups: Vec<Group<'a>>,
    /// Each error, in the order of the schema's errors.
    errors: Vec<Declared>,
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
                    ast::Definition::Errors(found) => {
                        let found = errors(found);
                        self.declare_group(found.body, found.public);
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
                    ast::Definition::Errors(_) | ast::Definition::Method(_) => {}
                }
            }
        }
        for index in 0..self.groups.len() {
            let declared = &self.groups[index].declared;
            let (file, scope, public) = (declared.file, declared.scope.clone(), declared.public);
            let own = self.groups[index].own;
            let own_errors = self.within(file, &scope, |checker| {
                checker.declare_errors(own, public, &mut errors_found)
            });
            self.groups[index].own_errors = own_errors;
        }
        for index in 0..self.groups.len() {
            // Reports unknown groups and cycles once, here.
            self.expand(index, &mut Vec::new());
        }
        for (index, file) in decoded.iter().enumerate() {
            self.file = index;
            self.scope = file.scope.clone();
            for definition in &file.document.definition {
                if let ast::Definition::Method(found) = definition {
                    let method = self.method(found, &mut errors_found);
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

    /// Declares the group an `ERRORS` defines, which starts with its name
    /// and its own errors.
    fn declare_group(&mut self, list: &'a ast::ErrorList, public: bool) {
        let Some(terms) = self.error_terms(list) else {
            return;
        };
        let Some(name) = terms.name else {
            self.error(
                error_terms::span(list),
                "found an `ERRORS` without a name before its errors; expected a group's name, its own errors in brackets, then any groups it joins with `+`, such as `ERRORS file errors [trashed, missing]`",
            );
            return;
        };
        self.check_words(&name, "an error group");
        if self
            .groups
            .iter()
            .any(|known| known.declared.scope == self.scope && known.declared.name == name.value)
        {
            self.error(
                name.span,
                format!(
                    "found a second error group named `{}`; expected each group once in a scope; rename one of them",
                    name.value
                ),
            );
        }
        let declared = self.declared(&name, public);
        self.groups.push(Group {
            declared,
            own: terms.own,
            own_errors: Vec::new(),
            joined: terms.groups,
            expanded: None,
        });
    }

    /// `list` read as its terms, or `None` after reporting what doesn't fit.
    fn error_terms(&mut self, list: &'a ast::ErrorList) -> Option<ErrorTerms<'a>> {
        match error_terms::read(list) {
            Ok(terms) => Some(terms),
            Err(Misplaced::NotATerm(span)) => {
                let written = self.written(span).to_string();
                self.error(
                    span,
                    format!(
                        "found `{written}` among errors; expected errors in brackets, then groups joined with `+`, such as `[missing (path: TEXT)] + file errors` or `[missing] + files/file errors`"
                    ),
                );
                None
            }
            Err(Misplaced::OwnAfterGroup(span)) => {
                self.error(
                    span,
                    "found errors in brackets after a `+`; expected the errors in brackets first, then the groups joined with `+`, such as `[missing] + file errors`",
                );
                None
            }
        }
    }

    /// Reads the errors defined in brackets, as indices into `found`, each
    /// once in a scope.
    fn declare_errors(
        &mut self,
        own: &[ast::Error],
        public: bool,
        found: &mut Vec<ErrorDefinition>,
    ) -> Vec<usize> {
        own.iter()
            .map(|error| {
                let shaped = self.shaped(error, "an error");
                let words = shaped.shape.words();
                if let Some(other) = self
                    .errors
                    .iter()
                    .find(|known| known.scope == self.scope && known.name == words)
                {
                    let line = anyx::line_column(&self.sources[other.file], other.span.start).line;
                    self.error(
                        error.name.span,
                        format!(
                            "found a second error `{words}` in {}, after the one on line {line}; expected each error once in a scope; to share one, put it in an `ERRORS` group and join the group with `+`",
                            self.this_scope()
                        ),
                    );
                }
                let declared = self.declared(
                    &ast::Spanned {
                        value: words,
                        span: error.name.span,
                    },
                    public,
                );
                self.errors.push(declared);
                found.push(ErrorDefinition {
                    scope: self.scope.clone(),
                    public,
                    error: shaped,
                });
                found.len() - 1
            })
            .collect()
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

    /// The group `reference` names, as an index into the groups.
    fn resolve_group(&mut self, reference: &GroupReference) -> Option<usize> {
        let full = !reference.scope.is_empty();
        let path = Path {
            scope: match full {
                true => reference.scope.clone(),
                false => self.scope.clone(),
            },
            name: reference.name.clone(),
            full,
        };
        let found = self.groups.iter().position(|known| {
            known.declared.scope == path.scope && known.declared.name == path.name
        });
        match found {
            Some(index) => {
                let public = self.groups[index].declared.public;
                self.check_access(reference.span, &path, public, "error group");
                Some(index)
            }
            None => {
                let an_error = self
                    .errors
                    .iter()
                    .any(|known| known.scope == path.scope && known.name == path.name);
                let message = match (path.full, an_error) {
                    (_, true) => format!(
                        "found `{}`, which is an error, not a group; expected a group after a `+`; put the error in an `ERRORS` group and join that group, since an error is defined once, in the brackets of the group or method that has it",
                        scoped(&path.scope, &path.name)
                    ),
                    (true, false) => format!(
                        "found `{}`, which names no error group; expected the full path of a `PUBLIC` `ERRORS` group, such as `files/file errors` for `PUBLIC ERRORS file errors [trashed, missing]` in `files.anyx`",
                        scoped(&path.scope, &path.name)
                    ),
                    (false, false) => format!(
                        "found `{}`, which is not an error group of {}; expected the name of an `ERRORS` group, such as `file errors` for `ERRORS file errors [trashed, missing]`{}",
                        path.name,
                        self.this_scope(),
                        self.elsewhere(self.groups.iter().map(|group| &group.declared), &path.name),
                    ),
                };
                self.error(reference.span, message);
                None
            }
        }
    }

    /// The errors that the group at `index` stands for, in order: its own,
    /// then those of the groups it joins. `None` when one of those can't be
    /// resolved. `visiting` holds the groups being expanded, to catch one
    /// that joins itself.
    fn expand(&mut self, index: usize, visiting: &mut Vec<usize>) -> Option<Vec<usize>> {
        if let Some(expanded) = &self.groups[index].expanded {
            return expanded.clone();
        }
        let group = &self.groups[index];
        let (file, scope, span, name) = (
            group.declared.file,
            group.declared.scope.clone(),
            group.declared.span,
            group.declared.name.clone(),
        );
        let (own, joined) = (group.own_errors.clone(), group.joined.clone());
        if visiting.contains(&index) {
            self.within(file, &scope, |checker| {
                checker.error(
                    span,
                    format!(
                        "found the error group `{name}` joining itself; expected a group to join only groups that don't lead back to it"
                    ),
                )
            });
            return None;
        }
        visiting.push(index);
        let expanded = self.within(file, &scope, |checker| {
            joined.iter().try_fold(own, |mut expanded, reference| {
                if let Some(found) = checker.resolve_group(reference) {
                    expanded.extend(checker.expand(found, visiting)?);
                }
                Some(expanded)
            })
        });
        visiting.pop();
        self.groups[index].expanded = Some(expanded.clone());
        expanded
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
            let shaped = self.shaped(variant, "a variant");
            if checked
                .iter()
                .any(|known| known.shape.words() == shaped.shape.words())
            {
                self.error(
                    variant.name.span,
                    format!(
                        "found a second variant named `{}`; expected each variant's words once; rename or remove one of them",
                        shaped.shape.words()
                    ),
                );
            }
            checked.push(shaped);
        }
        checked
    }

    /// A variant or an error: its doc, then its shape.
    fn shaped(&mut self, variant: &ast::Variant, what: &str) -> Shaped {
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
                    format!(
                        "found a string in {what}'s slot; expected a parameter, as in `(revision: INTEGER)`; {what}'s `#` doc goes on the lines before it"
                    ),
                ),
                ast::VariantSlot::Parameter(parameter) => parameters.push(parameter.clone()),
            }
        }
        let mut name = variant.name.clone();
        if doc.is_some() {
            name.parts.remove(0);
            name.text = name.text.trim_start_matches('_').trim_start().to_string();
        }
        let shape = self.shape(&name, &parameters, what);
        Shaped { doc, shape }
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

    fn method(&mut self, method: &'a ast::Method, found: &mut Vec<ErrorDefinition>) -> Method {
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
        let terms = list.and_then(|list| self.error_terms(list));
        let mut listed: Vec<Listed> = Vec::new();
        if let Some(terms) = terms {
            if let Some(name) = &terms.name {
                self.error(
                    name.span,
                    format!(
                        "found the name `{}` before a method's errors; expected its errors in brackets alone, since only an `ERRORS` group has a name, such as `ERRORS [missing] + file errors`",
                        name.value
                    ),
                );
            }
            let own = self.declare_errors(terms.own, false, found);
            listed.extend(terms.own.iter().zip(own).map(|(error, index)| Listed {
                index,
                span: error.name.span,
            }));
            for reference in &terms.groups {
                let Some(group) = self.resolve_group(reference) else {
                    continue;
                };
                let expanded = self.expand(group, &mut Vec::new()).unwrap_or_default();
                listed.extend(expanded.into_iter().map(|index| Listed {
                    index,
                    span: reference.span,
                }));
            }
        }
        let mut errors: Vec<usize> = Vec::new();
        for Listed { index, span } in listed {
            let declared = &self.errors[index];
            let this = scoped(&declared.scope, &declared.name);
            let same_words = errors
                .iter()
                .map(|known| &self.errors[*known])
                .find(|other| other.name == declared.name)
                .map(|other| scoped(&other.scope, &other.name));
            if errors.contains(&index) {
                let words = declared.name.clone();
                self.error(
                    span,
                    format!(
                        "found the error `{words}` twice among this method's errors, counting those of its groups; expected each error once; remove one"
                    ),
                );
            } else if let Some(other) = same_words {
                self.error(
                    span,
                    format!(
                        "found the errors `{other}` and `{this}` both listed for this method; expected errors of distinct words, since a response writes an error by its shape alone, without its scope; rename one of them"
                    ),
                );
            } else {
                errors.push(index);
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
