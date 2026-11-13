//! Writes the Rust for a checked grammar: a type for every rule, a decoder
//! for every type, and the support module they share.
//!
//! The work is in three passes. `Builder` plans the output: one [`Definition`]
//! per type or helper function, each named after the path of rule, field
//! and choice names leading to it. Then the fields that would make a type
//! contain itself are marked to be boxed. Then the plan is printed.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fmt::Write as _;

use anyx::Span;

use crate::diagnostic::Diagnostic;
use crate::model::{
    Choice, ContainerKind, Element, Grammar, LayoutRule, NumberType, Order, Pattern, Repetition,
    StringCondition, WordKind, values,
};

/// The module every generated file ends with.
const SUPPORT: &str = include_str!("support.rs");

/// The Rust source for `grammar`, not yet formatted.
pub fn generate(grammar: &Grammar) -> Result<String, Vec<Diagnostic>> {
    let mut builder = Builder {
        grammar,
        definitions: Vec::new(),
        rule_definitions: Vec::new(),
        regexes: Vec::new(),
        taken: BTreeSet::new(),
        diagnostics: Vec::new(),
    };
    builder.build();
    if !builder.diagnostics.is_empty() {
        return Err(builder.diagnostics);
    }
    builder.mark_boxes();
    let printer = Printer {
        documentation: grammar.documentation(),
        layouts: &grammar.layouts,
        definitions: &builder.definitions,
        rule_definitions: &builder.rule_definitions,
        names: Names::new(&builder.definitions),
    };
    Ok(printer.print(&builder.regexes))
}

/// A type, a decoder function, or both, named after `path`.
struct Definition {
    /// The words of the rule's name, then of each field or choice on the
    /// way: `string condition`, `starts with`.
    path: Vec<String>,
    /// Whether it is a rule, whose decoder is public.
    is_rule: bool,
    body: Body,
    /// The `#` line string above the rule, for its type.
    documentation: Option<String>,
}

enum Body {
    /// A rule that is another rule or a built-in: a type alias.
    Alias(Value),
    Struct(Record),
    /// A choice with fields: a struct-like variant of the enum `owner`, and
    /// the function that decodes it.
    Variant {
        owner: usize,
        name: String,
        record: Record,
    },
    Enum(Vec<Variant>),
    /// A bracket around at most one value. It has no type of its own: it
    /// decodes to that value with the bracket's span, or to the span alone.
    Transparent {
        kind: ContainerKind,
        steps: Vec<Step>,
    },
}

/// What a struct, or a variant with fields, is decoded from.
struct Record {
    source: Source,
    steps: Vec<Step>,
    /// With [`Order::Any`], every step is a field, and the record also
    /// keeps the nodes that fit none of them in `errors`.
    order: Order,
}

enum Source {
    Document,
    Container(ContainerKind),
    /// A call with fixed words. `expected` describes it in errors.
    Call {
        expected: String,
    },
    /// Any call whose name passes `condition`, a closure. When not
    /// `decoded`, its arguments are kept as raw nodes.
    AnyCall {
        condition: String,
        expected: String,
        decoded: bool,
    },
}

/// One step of matching a sequence.
enum Step {
    /// A word that is only matched.
    Word(String),
    Field(Field),
}

struct Field {
    /// The Rust identifier.
    name: String,
    repetition: Repetition,
    value: Value,
    /// What an error calls the value when it is missing.
    expected: String,
    /// The element's name in the grammar, for errors about it.
    label: String,
    boxed: bool,
    documentation: Option<String>,
}

struct Variant {
    name: String,
    payload: Payload,
    documentation: Option<String>,
}

enum Payload {
    /// The word alone: the variant holds its span.
    Word(String),
    Value {
        value: Value,
        boxed: bool,
    },
    /// The definition holding the variant's fields.
    Record(usize),
}

/// What one node decodes to.
#[derive(Clone)]
enum Value {
    /// The type of a rule, by its index in the grammar.
    Rule(usize),
    Definition(usize),
    /// The span of a word.
    Word(String),
    /// The name words a `WORDS` takes, whose text must pass `condition`,
    /// a closure. Only a call's sequence decodes them.
    Words {
        condition: String,
    },
    /// A built-in, decoded by the expression `call`.
    Leaf {
        kind: Leaf,
        call: String,
    },
}

#[derive(Clone, Copy)]
enum Leaf {
    StringLiteral,
    Text,
    Number(&'static str),
    Node,
}

/// How to get the span of a decoded value.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    /// `value.span`
    Field,
    /// `value.span()`
    Method,
    /// The value is a span.
    Itself,
}

/// `///` lines for `documentation`, or nothing.
fn doc_comment(documentation: Option<&str>) -> String {
    documentation
        .map(|text| {
            text.lines()
                .map(|line| match line {
                    "" => "///\n".to_string(),
                    line => format!("/// {line}\n"),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `///` lines for the field of `record` named `field`, or nothing.
fn field_documentation(record: &Record, field: &str) -> String {
    let documentation = record.steps.iter().find_map(|step| match step {
        Step::Field(known) if known.name == field => known.documentation.as_deref(),
        _ => None,
    });
    doc_comment(documentation)
}

fn words(name: &str) -> Vec<String> {
    name.split_whitespace().map(str::to_string).collect()
}

fn pascal_case(path: &[String]) -> String {
    path.iter()
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn snake_case(path: &[String]) -> String {
    path.join("_")
}

/// Words Rust reserves, which a field can only be named with `r#`.
const RUST_KEYWORDS: [&str; 51] = [
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn",
    "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
    "virtual", "where", "while", "yield", "union", "safe", "raw",
];

fn field_identifier(name: &str) -> String {
    let identifier = snake_case(&words(name));
    match RUST_KEYWORDS.contains(&identifier.as_str()) {
        true => format!("r#{identifier}"),
        false => identifier,
    }
}

struct Builder<'a> {
    grammar: &'a Grammar,
    definitions: Vec<Definition>,
    /// The definition of each rule, in the grammar's order.
    rule_definitions: Vec<usize>,
    /// The regexes of the grammar: the first is `REGEX_1`.
    regexes: Vec<String>,
    /// The paths that already name a definition.
    taken: BTreeSet<Vec<String>>,
    diagnostics: Vec<Diagnostic>,
}

impl Builder<'_> {
    fn build(&mut self) {
        let grammar = self.grammar;
        let path = vec!["document".to_string()];
        let document = self.reserve(&path, true, Span::empty(0));
        let steps = self.steps(&grammar.entry, &path, "item");
        self.definitions[document].body = Body::Struct(Record {
            source: Source::Document,
            steps,
            order: grammar.entry_order,
        });
        for rule in &grammar.rules {
            let path = words(&rule.name);
            let definition = match &rule.pattern {
                Pattern::Reference(_)
                | Pattern::Word(_)
                | Pattern::AnyWord { .. }
                | Pattern::String(_)
                | Pattern::Number(_)
                | Pattern::AnyNode => {
                    let definition = self.reserve(&path, true, rule.name_span);
                    let value = self.value(&rule.pattern, &path, rule.name_span, true);
                    self.definitions[definition].body = Body::Alias(value);
                    definition
                }
                pattern => match self.value(pattern, &path, rule.name_span, true) {
                    Value::Definition(definition) => definition,
                    _ => unreachable!("a rule with a body of its own gets a definition"),
                },
            };
            if self.definitions[definition].documentation.is_none() {
                self.definitions[definition].documentation = rule.documentation.clone();
            }
            self.rule_definitions.push(definition);
        }
    }

    /// Adds an empty definition named after `path`, to fill once what it
    /// holds is planned, so that it comes before them in the output.
    fn reserve(&mut self, path: &[String], is_rule: bool, span: Span) -> usize {
        if !self.taken.insert(path.to_vec()) {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "found a second part of the grammar that becomes the Rust type `{}`, since the names leading to it are `{}` again; expected a rule's name to differ from another rule's name followed by one of its fields or choices; rename one of them",
                    pascal_case(path),
                    path.join(" ")
                ),
            ));
        }
        self.definitions.push(Definition {
            path: path.to_vec(),
            is_rule,
            body: Body::Enum(Vec::new()),
            documentation: None,
        });
        self.definitions.len() - 1
    }

    /// What `pattern` decodes to, adding the definitions it needs. Those
    /// are named after `path`.
    fn value(&mut self, pattern: &Pattern, path: &[String], span: Span, is_rule: bool) -> Value {
        let expected = self.grammar.describe_one(pattern);
        match pattern {
            Pattern::Reference(rule) => Value::Rule(*rule),
            Pattern::OneOf(choices) => {
                let definition = self.reserve(path, is_rule, span);
                let variants = choices
                    .iter()
                    .map(|choice| self.variant(definition, choice, path))
                    .collect();
                self.definitions[definition].body = Body::Enum(variants);
                Value::Definition(definition)
            }
            Pattern::Container {
                kind,
                elements,
                order,
            } => {
                let definition = self.reserve(path, is_rule, span);
                let transparent = is_transparent(elements) && !is_rule && *order == Order::Written;
                self.definitions[definition].body = if transparent {
                    Body::Transparent {
                        kind: *kind,
                        steps: self.steps(elements, path, "item"),
                    }
                } else {
                    Body::Struct(self.record(pattern, path))
                };
                Value::Definition(definition)
            }
            Pattern::SpecificCall(_) | Pattern::AnyCall { .. } => {
                let definition = self.reserve(path, is_rule, span);
                self.definitions[definition].body = Body::Struct(self.record(pattern, path));
                Value::Definition(definition)
            }
            Pattern::Word(text) => Value::Word(text.clone()),
            Pattern::AnyWord { kind, conditions } => {
                let mut terms: Vec<String> = match kind {
                    None => Vec::new(),
                    Some(WordKind::Keyword) => vec!["anyx::is_keyword(text)".to_string()],
                    Some(WordKind::Symbol) => {
                        vec!["anyx::is_symbol_word(text)".to_string()]
                    }
                    Some(WordKind::Name) => vec![
                        "!anyx::is_keyword(text)".to_string(),
                        "!anyx::is_symbol_word(text)".to_string(),
                    ],
                };
                terms.extend(self.terms(conditions, "text"));
                Value::Leaf {
                    kind: Leaf::Text,
                    call: format!(
                        "support::any_word(node, {}, {expected:?})",
                        closure(&terms, "text")
                    ),
                }
            }
            Pattern::Words(conditions) => {
                let terms = self.terms(conditions, "text");
                Value::Words {
                    condition: closure(&terms, "text"),
                }
            }
            Pattern::AnyNode => Value::Leaf {
                kind: Leaf::Node,
                call: "support::any_node(node)".to_string(),
            },
            Pattern::String(conditions) => {
                let terms = self.terms(conditions, "text");
                Value::Leaf {
                    kind: Leaf::StringLiteral,
                    call: format!(
                        "support::string(node, {}, {expected:?})",
                        closure(&terms, "text")
                    ),
                }
            }
            Pattern::Number(number) => match number.number_type() {
                NumberType::Integer {
                    rust,
                    minimum,
                    maximum,
                } => Value::Leaf {
                    kind: Leaf::Number(rust),
                    call: format!(
                        "support::integer::<{rust}>(node, {}, {}, {}, {expected:?})",
                        integer_text(minimum),
                        integer_text(maximum),
                        number.signed
                    ),
                },
                NumberType::Float { minimum, maximum } => Value::Leaf {
                    kind: Leaf::Number("f64"),
                    call: format!(
                        "support::float(node, {}, {}, {}, {expected:?})",
                        float_text(minimum),
                        float_text(maximum),
                        number.signed
                    ),
                },
            },
        }
    }

    /// The fields of a struct, or of a variant, decoded from `pattern`.
    fn record(&mut self, pattern: &Pattern, path: &[String]) -> Record {
        let expected = self.grammar.describe_one(pattern);
        match pattern {
            Pattern::Container {
                kind,
                elements,
                order,
            } => Record {
                source: Source::Container(*kind),
                steps: self.steps(elements, path, "item"),
                order: *order,
            },
            Pattern::SpecificCall(parts) => Record {
                source: Source::Call { expected },
                steps: self.steps(parts, path, "item"),
                order: Order::Written,
            },
            Pattern::AnyCall { name, arguments } => {
                let terms = self.terms(name, "name");
                Record {
                    source: Source::AnyCall {
                        condition: closure(&terms, "name"),
                        expected,
                        decoded: arguments.is_some(),
                    },
                    steps: match arguments {
                        Some(arguments) => {
                            self.steps(std::slice::from_ref(&**arguments), path, "arguments")
                        }
                        None => Vec::new(),
                    },
                    order: Order::Written,
                }
            }
            _ => unreachable!("only brackets and calls have fields"),
        }
    }

    /// The steps matching `elements`. An element with no name is called
    /// `unnamed`.
    fn steps(&mut self, elements: &[Element], path: &[String], unnamed: &str) -> Vec<Step> {
        elements
            .iter()
            .map(|element| {
                if let (true, Pattern::Word(text)) = (element.is_bare_word(), &element.pattern) {
                    return Step::Word(text.clone());
                }
                let name = element.name.as_deref().unwrap_or(unnamed);
                let mut path = path.to_vec();
                path.extend(words(name));
                Step::Field(Field {
                    name: field_identifier(name),
                    repetition: element.repetition,
                    value: self.value(&element.pattern, &path, element.span, false),
                    expected: self.grammar.describe_one(&element.pattern),
                    label: name.to_string(),
                    boxed: false,
                    documentation: element.documentation.clone(),
                })
            })
            .collect()
    }

    fn variant(&mut self, owner: usize, choice: &Choice, path: &[String]) -> Variant {
        let mut path = path.to_vec();
        path.extend(words(&choice.name));
        let name = pascal_case(&words(&choice.name));
        let has_fields = match &choice.pattern {
            Pattern::SpecificCall(_) | Pattern::AnyCall { .. } => true,
            Pattern::Container { elements, .. } => !is_transparent(elements),
            _ => false,
        };
        let payload = match &choice.pattern {
            Pattern::Word(text) => Payload::Word(text.clone()),
            pattern if has_fields => {
                let definition = self.reserve(&path, false, choice.span);
                let record = self.record(pattern, &path);
                self.definitions[definition].body = Body::Variant {
                    owner,
                    name: name.clone(),
                    record,
                };
                Payload::Record(definition)
            }
            pattern => Payload::Value {
                value: self.value(pattern, &path, choice.span, false),
                boxed: false,
            },
        };
        Variant {
            name,
            payload,
            documentation: choice.documentation.clone(),
        }
    }

    /// One Rust condition on `variable` for each of `conditions`.
    fn terms(&mut self, conditions: &[StringCondition], variable: &str) -> Vec<String> {
        conditions
            .iter()
            .map(|condition| match condition {
                StringCondition::Is(text) => format!("{variable} == {text:?}"),
                StringCondition::StartsWith(text) => format!("{variable}.starts_with({text:?})"),
                StringCondition::EndsWith(text) => format!("{variable}.ends_with({text:?})"),
                StringCondition::Contains(text) => format!("{variable}.contains({text:?})"),
                StringCondition::MatchingRegex(regex) => {
                    let index = match self.regexes.iter().position(|known| known == regex) {
                        Some(index) => index,
                        None => {
                            self.regexes.push(regex.clone());
                            self.regexes.len() - 1
                        }
                    };
                    format!("REGEX_{}.is_match({variable})", index + 1)
                }
                StringCondition::NoSlots => {
                    format!("!{variable}.split(' ').any(|word| word == \"_\")")
                }
                StringCondition::NoKeyword => {
                    format!("!{variable}.split(' ').any(anyx::is_keyword)")
                }
            })
            .collect()
    }

    /// The types `value` holds directly, with no `Vec` in between.
    fn held_types(&self, value: &Value) -> Vec<usize> {
        let definition = match value {
            Value::Rule(rule) => self.rule_definitions[*rule],
            Value::Definition(definition) => *definition,
            Value::Word(_) | Value::Words { .. } | Value::Leaf { .. } => return Vec::new(),
        };
        match &self.definitions[definition].body {
            Body::Alias(value) => self.held_types(value),
            Body::Struct(_) | Body::Enum(_) => vec![definition],
            Body::Variant { owner, .. } => vec![*owner],
            Body::Transparent { steps, .. } => self.held_by_steps(steps).concat(),
        }
    }

    /// For each step, the types it holds directly.
    fn held_by_steps(&self, steps: &[Step]) -> Vec<Vec<usize>> {
        steps
            .iter()
            .map(|step| match step {
                Step::Field(field) if !field.repetition.may_repeat() => {
                    self.held_types(&field.value)
                }
                _ => Vec::new(),
            })
            .collect()
    }

    /// The places of the type `definition` that hold a value directly: the
    /// definition and step (or variant) of each, and the types held there.
    fn slots(&self, definition: usize) -> Vec<((usize, usize), Vec<usize>)> {
        let of_record = |definition: usize, record: &Record| {
            let held = self.held_by_steps(&record.steps);
            held.into_iter()
                .enumerate()
                .map(move |(step, held)| ((definition, step), held))
        };
        match &self.definitions[definition].body {
            Body::Struct(record) => of_record(definition, record).collect(),
            Body::Enum(variants) => variants
                .iter()
                .enumerate()
                .flat_map(|(index, variant)| match &variant.payload {
                    Payload::Word(_) => Vec::new(),
                    Payload::Value { value, .. } => {
                        vec![((definition, index), self.held_types(value))]
                    }
                    Payload::Record(fields) => match &self.definitions[*fields].body {
                        Body::Variant { record, .. } => of_record(*fields, record).collect(),
                        _ => unreachable!("a variant's fields are a variant definition"),
                    },
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Marks the fields to box, so that no type contains itself: walking
    /// the types depth first, a field leading back to a type still being
    /// walked closes a cycle.
    fn mark_boxes(&mut self) {
        #[derive(Clone, Copy, PartialEq)]
        enum State {
            Unvisited,
            Open,
            Done,
        }
        fn walk(
            builder: &Builder,
            definition: usize,
            states: &mut [State],
            boxed: &mut Vec<(usize, usize)>,
        ) {
            states[definition] = State::Open;
            for (slot, held) in builder.slots(definition) {
                for target in held {
                    match states[target] {
                        State::Open => boxed.push(slot),
                        State::Unvisited => walk(builder, target, states, boxed),
                        State::Done => {}
                    }
                }
            }
            states[definition] = State::Done;
        }
        let mut states = vec![State::Unvisited; self.definitions.len()];
        let mut boxed = Vec::new();
        for definition in 0..self.definitions.len() {
            let is_type = matches!(
                self.definitions[definition].body,
                Body::Struct(_) | Body::Enum(_)
            );
            if is_type && states[definition] == State::Unvisited {
                walk(self, definition, &mut states, &mut boxed);
            }
        }
        for (definition, index) in boxed {
            match &mut self.definitions[definition].body {
                Body::Struct(record) | Body::Variant { record, .. } => {
                    if let Step::Field(field) = &mut record.steps[index] {
                        field.boxed = true;
                    }
                }
                Body::Enum(variants) => {
                    if let Payload::Value { boxed, .. } = &mut variants[index].payload {
                        *boxed = true;
                    }
                }
                _ => {}
            }
        }
    }
}

/// Whether a bracket of `elements` needs no struct: it has at most one
/// value, and that value has no name.
fn is_transparent(elements: &[Element]) -> bool {
    values(elements).all(|element| element.name.is_none())
}

/// `|variable| first && second`, or `|_| true` with no terms.
fn closure(terms: &[String], variable: &str) -> String {
    let argument = format!("({variable})");
    match terms {
        [] => "|_| true".to_string(),
        // `|text| is_keyword(text)` is the function itself.
        [only] if only.ends_with(&argument) && !only.contains(['!', '.']) => {
            only[..only.len() - argument.len()].to_string()
        }
        _ => format!("|{variable}| {}", terms.join(" && ")),
    }
}

fn integer_text(value: i128) -> String {
    const NAMED: [(i128, &str); 5] = [
        (i128::MIN, "i128::MIN"),
        (i128::MAX, "i128::MAX"),
        (i64::MIN as i128, "i128::from(i64::MIN)"),
        (i64::MAX as i128, "i128::from(i64::MAX)"),
        (u64::MAX as i128, "i128::from(u64::MAX)"),
    ];
    match NAMED.iter().find(|(named, _)| *named == value) {
        Some((_, text)) => text.to_string(),
        None => value.to_string(),
    }
}

fn float_text(value: f64) -> String {
    if value == f64::NEG_INFINITY {
        "f64::NEG_INFINITY".to_string()
    } else if value == f64::INFINITY {
        "f64::INFINITY".to_string()
    } else {
        format!("{value:?}")
    }
}

/// The names the generated code uses for types that are not the grammar's.
/// A grammar may have a rule called `string` or `node`, whose type takes the
/// short name; the other is then written by its full path.
struct Names {
    shadowed: BTreeSet<&'static str>,
    used: RefCell<BTreeSet<&'static str>>,
}

/// The short name, the full path, and whether the short name needs a `use`.
const KNOWN_NAMES: [(&str, &str, bool); 11] = [
    ("Box", "std::boxed::Box", false),
    ("Option", "std::option::Option", false),
    ("Result", "std::result::Result", false),
    ("String", "std::string::String", false),
    ("Vec", "std::vec::Vec", false),
    ("Node", "anyx::Node", true),
    ("Span", "anyx::Span", true),
    ("StringLiteral", "anyx::StringLiteral", true),
    ("DecodeError", "support::DecodeError", false),
    ("Spanned", "support::Spanned", false),
    ("CallName", "support::CallName", false),
];

impl Names {
    fn new(definitions: &[Definition]) -> Self {
        let types: BTreeSet<String> = definitions
            .iter()
            .map(|definition| pascal_case(&definition.path))
            .collect();
        Names {
            shadowed: KNOWN_NAMES
                .iter()
                .map(|(short, _, _)| *short)
                .filter(|short| types.contains(*short))
                .collect(),
            used: RefCell::new(BTreeSet::new()),
        }
    }

    fn get(&self, name: &str) -> &'static str {
        let (short, full, _) = KNOWN_NAMES
            .iter()
            .find(|(short, _, _)| *short == name)
            .expect("a known name");
        match self.shadowed.contains(short) {
            true => full,
            false => {
                self.used.borrow_mut().insert(short);
                short
            }
        }
    }
}

struct Printer<'a> {
    /// Each expectation with its documentation.
    documentation: Vec<(String, String)>,
    definitions: &'a [Definition],
    rule_definitions: &'a [usize],
    names: Names,
    layouts: &'a [LayoutRule],
}

impl Printer<'_> {
    fn print(&self, regexes: &[String]) -> String {
        let mut body = String::new();
        for (index, regex) in regexes.iter().enumerate() {
            writeln!(
                body,
                "static REGEX_{}: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {{ regex::Regex::new({regex:?}).expect(\"the generator checked it\") }});\n",
                index + 1
            )
            .unwrap();
        }
        for definition in self.definitions {
            self.print_type(&mut body, definition);
        }
        for definition in self.definitions {
            self.print_span(&mut body, definition);
        }
        for definition in self.definitions {
            self.print_errors(&mut body, definition);
        }
        for definition in self.definitions {
            self.print_decoder(&mut body, definition);
        }
        body.push_str(
            "/// Parses `source` with anyx and decodes its document.\npub fn parse_document(source: &str) -> support::Parsed<Document> {\nsupport::parse(source, decode_document)\n}\n\n",
        );
        let documentation: String = self
            .documentation
            .iter()
            .map(|(expected, text)| format!("({expected:?}, {text:?}),\n"))
            .collect();
        writeln!(
            body,
            "/// What the grammar's `#` line strings say about what may be written,\n/// by what errors call it: ``the call `title _` ``, ``the word `yes` ``.\npub const DOCUMENTATION: &[(&str, &str)] = &[\n{documentation}];\n"
        )
        .unwrap();
        let quoted = |words: &[String]| {
            words
                .iter()
                .map(|word| format!("{word:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let layouts: String = self
            .layouts
            .iter()
            .map(|layout| {
                format!(
                    "support::LayoutRule {{ words: &[{}], break_before: &[{}] }},\n",
                    quoted(&layout.words),
                    quoted(&layout.break_before)
                )
            })
            .collect();
        writeln!(
            body,
            "/// How [`format`] lays out calls, from the grammar's `FORMAT`s.\npub const LAYOUT: &[support::LayoutRule] = &[\n{layouts}];\n\n/// Formats `source` canonically, laying calls out as the grammar says.\npub fn format(source: &str) -> std::result::Result<std::string::String, std::vec::Vec<anyx::Error>> {{\nsupport::format(source, LAYOUT)\n}}\n"
        )
        .unwrap();

        let mut output = String::from(
            "// Generated by anyx-generate. To change it, change the grammar and generate it again.\n\n#![allow(dead_code)]\n\n",
        );
        let used = self.names.used.borrow();
        let imported: Vec<&str> = KNOWN_NAMES
            .iter()
            .filter(|(short, _, import)| *import && used.contains(short))
            .map(|(short, _, _)| *short)
            .collect();
        if !imported.is_empty() {
            writeln!(output, "use anyx::{{{}}};", imported.join(", ")).unwrap();
        }
        // `CallName` only appears with `ANYFIX CALL`.
        let exported: Vec<&str> = ["CallName", "DecodeError", "Spanned"]
            .into_iter()
            .filter(|short| !self.names.shadowed.contains(short))
            .filter(|short| *short != "CallName" || used.contains(short))
            .collect();
        if !exported.is_empty() {
            writeln!(output, "pub use support::{{{}}};", exported.join(", ")).unwrap();
        }
        output.push('\n');
        output.push_str(&body);
        output.push_str("pub mod support {\n");
        output.push_str(SUPPORT);
        output.push_str("}\n");
        output
    }

    fn type_name(&self, definition: usize) -> String {
        pascal_case(&self.definitions[definition].path)
    }

    fn function_name(&self, definition: usize) -> String {
        format!("decode_{}", snake_case(&self.definitions[definition].path))
    }

    fn value_definition(&self, value: &Value) -> Option<usize> {
        match value {
            Value::Rule(rule) => Some(self.rule_definitions[*rule]),
            Value::Definition(definition) => Some(*definition),
            Value::Word(_) | Value::Words { .. } | Value::Leaf { .. } => None,
        }
    }

    fn value_type(&self, value: &Value) -> String {
        let spanned = |inner: &str| format!("{}<{inner}>", self.names.get("Spanned"));
        match value {
            Value::Word(_) => self.names.get("Span").to_string(),
            Value::Words { .. } => spanned(self.names.get("String")),
            Value::Leaf { kind, .. } => match kind {
                Leaf::StringLiteral => spanned(self.names.get("StringLiteral")),
                Leaf::Text => spanned(self.names.get("String")),
                Leaf::Number(rust) => spanned(rust),
                Leaf::Node => self.names.get("Node").to_string(),
            },
            Value::Rule(rule) => self.type_name(self.rule_definitions[*rule]),
            Value::Definition(definition) => match &self.definitions[*definition].body {
                Body::Transparent { steps, .. } => match only_field(steps) {
                    Some(field) => spanned(&self.field_type(field)),
                    None => self.names.get("Span").to_string(),
                },
                _ => self.type_name(*definition),
            },
        }
    }

    fn boxed_type(&self, value: &Value, boxed: bool) -> String {
        let inner = self.value_type(value);
        match boxed {
            true => format!("{}<{inner}>", self.names.get("Box")),
            false => inner,
        }
    }

    fn field_type(&self, field: &Field) -> String {
        let inner = self.boxed_type(&field.value, field.boxed);
        match field.repetition {
            Repetition::One => inner,
            Repetition::Optional => format!("{}<{inner}>", self.names.get("Option")),
            Repetition::ZeroOrMore | Repetition::OneOrMore => {
                format!("{}<{inner}>", self.names.get("Vec"))
            }
            Repetition::Each => format!(
                "{}<{}<{inner}, {}>>",
                self.names.get("Vec"),
                self.names.get("Result"),
                self.names.get("DecodeError")
            ),
        }
    }

    fn shape(&self, value: &Value) -> Shape {
        let Some(definition) = self.value_definition(value) else {
            return match value {
                Value::Word(_) => Shape::Itself,
                _ => Shape::Field,
            };
        };
        match &self.definitions[definition].body {
            Body::Alias(value) => self.shape(value),
            Body::Struct(_) => Shape::Field,
            Body::Enum(_) | Body::Variant { .. } => Shape::Method,
            Body::Transparent { steps, .. } => match only_field(steps) {
                Some(_) => Shape::Field,
                None => Shape::Itself,
            },
        }
    }

    /// The expression decoding `node` to `value`.
    fn call(&self, value: &Value) -> String {
        match value {
            Value::Word(text) => format!("support::word(node, {text:?})"),
            Value::Words { .. } => unreachable!("only a call's sequence decodes `WORDS`"),
            Value::Leaf { call, .. } => call.clone(),
            Value::Rule(rule) => {
                format!("{}(node)", self.function_name(self.rule_definitions[*rule]))
            }
            Value::Definition(definition) => format!("{}(node)", self.function_name(*definition)),
        }
    }

    fn boxed_call(&self, value: &Value, boxed: bool) -> String {
        match boxed {
            true => format!("{}.map({}::new)", self.call(value), self.names.get("Box")),
            false => self.call(value),
        }
    }

    /// A function decoding a node to `value`: a name or a closure.
    fn decoder(&self, value: &Value, boxed: bool) -> String {
        match (self.value_definition(value), boxed) {
            (Some(definition), false) => self.function_name(definition),
            // Taking only the node, it needs no closure around it.
            (None, false)
                if matches!(
                    value,
                    Value::Leaf {
                        kind: Leaf::Node,
                        ..
                    }
                ) =>
            {
                "support::any_node".to_string()
            }
            _ => format!("|node| {}", self.boxed_call(value, boxed)),
        }
    }

    /// The fields of a struct or variant, without its span.
    fn record_fields(&self, record: &Record) -> Vec<(String, String)> {
        let mut fields = Vec::new();
        if let Source::AnyCall { decoded, .. } = &record.source {
            fields.push(("name".to_string(), self.names.get("CallName").to_string()));
            if !decoded {
                let arguments = format!("{}<{}>", self.names.get("Vec"), self.names.get("Node"));
                fields.push(("arguments".to_string(), arguments));
            }
        }
        for step in &record.steps {
            if let Step::Field(field) = step {
                fields.push((field.name.clone(), self.field_type(field)));
            }
        }
        if record.order == Order::Any {
            let errors = format!(
                "{}<{}>",
                self.names.get("Vec"),
                self.names.get("DecodeError")
            );
            fields.push(("errors".to_string(), errors));
        }
        fields
    }

    fn print_type(&self, output: &mut String, definition: &Definition) {
        let name = pascal_case(&definition.path);
        let derive = "#[derive(Clone, Debug, PartialEq)]";
        let span = self.names.get("Span");
        if matches!(
            definition.body,
            Body::Alias(_) | Body::Struct(_) | Body::Enum(_)
        ) {
            output.push_str(&doc_comment(definition.documentation.as_deref()));
        }
        match &definition.body {
            Body::Alias(value) => {
                writeln!(output, "pub type {name} = {};\n", self.value_type(value)).unwrap();
            }
            Body::Struct(record) => {
                writeln!(output, "{derive}\npub struct {name} {{").unwrap();
                for (field, field_type) in self.record_fields(record) {
                    let documentation = field_documentation(record, &field);
                    writeln!(output, "{documentation}pub {field}: {field_type},").unwrap();
                }
                writeln!(output, "pub span: {span},\n}}\n").unwrap();
            }
            Body::Enum(variants) => {
                writeln!(output, "{derive}\npub enum {name} {{").unwrap();
                for variant in variants {
                    output.push_str(&doc_comment(variant.documentation.as_deref()));
                    match &variant.payload {
                        Payload::Word(_) => writeln!(output, "{}({span}),", variant.name),
                        Payload::Value { value, boxed } => writeln!(
                            output,
                            "{}({}),",
                            variant.name,
                            self.boxed_type(value, *boxed)
                        ),
                        Payload::Record(fields) => {
                            let Body::Variant { record, .. } = &self.definitions[*fields].body
                            else {
                                unreachable!("a variant's fields are a variant definition");
                            };
                            write!(output, "{} {{", variant.name).unwrap();
                            for (field, field_type) in self.record_fields(record) {
                                let documentation = field_documentation(record, &field);
                                write!(output, "{documentation}{field}: {field_type}, ").unwrap();
                            }
                            writeln!(output, "span: {span} }},")
                        }
                    }
                    .unwrap();
                }
                output.push_str("}\n\n");
            }
            Body::Variant { .. } | Body::Transparent { .. } => {}
        }
    }

    /// The [`support::Errors`] of a struct or an enum: the errors in each
    /// of its fields, in order.
    fn print_errors(&self, output: &mut String, definition: &Definition) {
        let name = pascal_case(&definition.path);
        let visit = |fields: &[(String, String)], prefix: &str| -> String {
            fields
                .iter()
                .map(|(field, _)| format!("support::Errors::errors({prefix}{field}, found);\n"))
                .collect()
        };
        let body = match &definition.body {
            Body::Struct(record) => visit(&self.record_fields(record), "&self."),
            Body::Enum(variants) => {
                let mut arms = String::new();
                for variant in variants {
                    let variant_name = &variant.name;
                    match &variant.payload {
                        Payload::Word(_) => {
                            writeln!(arms, "{name}::{variant_name}(_) => {{}}").unwrap();
                        }
                        Payload::Value { .. } => writeln!(
                            arms,
                            "{name}::{variant_name}(value) => support::Errors::errors(value, found),"
                        )
                        .unwrap(),
                        Payload::Record(fields) => {
                            let Body::Variant { record, .. } = &self.definitions[*fields].body
                            else {
                                unreachable!("a variant's fields are a variant definition");
                            };
                            let fields = self.record_fields(record);
                            let bound: String = fields
                                .iter()
                                .map(|(field, _)| format!("{field}, "))
                                .collect();
                            writeln!(
                                arms,
                                "{name}::{variant_name} {{ {bound}.. }} => {{\n{}}}",
                                visit(&fields, "")
                            )
                            .unwrap();
                        }
                    }
                }
                format!("match self {{\n{arms}}}\n")
            }
            Body::Alias(_) | Body::Variant { .. } | Body::Transparent { .. } => return,
        };
        // A type with nothing inside that holds errors never uses `found`.
        let found = match body.contains("found") {
            true => "found",
            false => "_",
        };
        writeln!(
            output,
            "impl support::Errors for {name} {{\nfn errors<'a>(&'a self, {found}: &mut std::vec::Vec<&'a support::DecodeError>) {{\n{body}}}\n}}\n"
        )
        .unwrap();
    }

    /// The `span` method of an enum.
    fn print_span(&self, output: &mut String, definition: &Definition) {
        let Body::Enum(variants) = &definition.body else {
            return;
        };
        let name = pascal_case(&definition.path);
        let mut spans = Vec::new();
        let mut arms = Vec::new();
        for variant in variants {
            let variant_name = &variant.name;
            match &variant.payload {
                Payload::Word(_) => spans.push(format!("{name}::{variant_name}(span)")),
                Payload::Record(_) => spans.push(format!("{name}::{variant_name} {{ span, .. }}")),
                Payload::Value { value, .. } => match self.shape(value) {
                    Shape::Itself => spans.push(format!("{name}::{variant_name}(span)")),
                    Shape::Field => {
                        arms.push(format!("{name}::{variant_name}(value) => value.span,"));
                    }
                    Shape::Method => {
                        arms.push(format!("{name}::{variant_name}(value) => value.span(),"));
                    }
                },
            }
        }
        if !spans.is_empty() {
            arms.insert(0, format!("{} => *span,", spans.join(" | ")));
        }
        writeln!(
            output,
            "impl {name} {{\npub fn span(&self) -> {} {{\nmatch self {{\n{}\n}}\n}}\n}}\n",
            self.names.get("Span"),
            arms.join("\n")
        )
        .unwrap();
    }

    /// The field expressions matching `steps` on the sequence `variable`,
    /// and the statement matching the words after the last field.
    fn sequence(&self, variable: &str, steps: &[Step]) -> (Vec<(String, String)>, String) {
        let mut fields = Vec::new();
        let mut words = String::new();
        for step in steps {
            let field = match step {
                Step::Word(text) => {
                    write!(words, ".word({text:?})?").unwrap();
                    continue;
                }
                Step::Field(field) => field,
            };
            let decoder = || self.decoder(&field.value, field.boxed);
            let expected = &field.expected;
            let matched = match (&field.value, field.repetition) {
                (Value::Word(text), Repetition::One) => format!(".word_span({text:?})?"),
                (Value::Words { condition }, _) => format!(".words({condition}, {expected:?})?"),
                (_, Repetition::One) => format!(".one({}, {expected:?})?", decoder()),
                (_, Repetition::Optional) => format!(".optional({})", decoder()),
                (_, Repetition::ZeroOrMore) => format!(".zero_or_more({})", decoder()),
                (_, Repetition::OneOrMore) => format!(".one_or_more({}, {expected:?})?", decoder()),
                (_, Repetition::Each) => format!(".each({})", decoder()),
            };
            fields.push((field.name.clone(), format!("{variable}{words}{matched}")));
            words.clear();
        }
        let rest = match words.is_empty() {
            true => String::new(),
            false => format!("{variable}{words};\n"),
        };
        (fields, rest)
    }

    fn open_container(kind: ContainerKind) -> &'static str {
        match kind {
            ContainerKind::CurlyBrace => "let mut elements = support::curly_brace(node)?;",
            ContainerKind::SquareBracket => "let mut elements = support::square_bracket(node)?;",
            ContainerKind::Parenthesis => "let mut elements = support::parenthesis(node)?;",
        }
    }

    /// The body of a function decoding `record` into `constructor`.
    fn print_record(&self, output: &mut String, record: &Record, constructor: &str) {
        let mut span = "node.span";
        let mut first_fields = "";
        let variable = match &record.source {
            Source::Document => {
                output.push_str("let mut elements = support::document(nodes);\n");
                span = "support::document_span(nodes)";
                "elements"
            }
            Source::Container(kind) => {
                writeln!(output, "{}", Self::open_container(*kind)).unwrap();
                "elements"
            }
            Source::Call { expected } => {
                writeln!(
                    output,
                    "let mut parts = support::call(node, {expected:?})?;"
                )
                .unwrap();
                "parts"
            }
            Source::AnyCall {
                condition,
                expected,
                decoded,
            } => {
                writeln!(
                    output,
                    "let (name, arguments) = support::any_call(node, {condition}, {expected:?})?;"
                )
                .unwrap();
                if !decoded {
                    writeln!(
                        output,
                        "Ok({constructor} {{ name, arguments: arguments.to_vec(), span: node.span }})"
                    )
                    .unwrap();
                    return;
                }
                output.push_str("let mut arguments = support::arguments(node, arguments);\n");
                first_fields = "name, ";
                "arguments"
            }
        };
        if record.order == Order::Any {
            self.any_order(output, variable, &record.steps);
            write!(output, "Ok({constructor} {{ {first_fields}").unwrap();
            for (name, expression) in self.any_order_fields(variable, &record.steps) {
                write!(output, "{name}: {expression}, ").unwrap();
            }
            writeln!(output, "errors, span: {span} }})").unwrap();
            return;
        }
        let (fields, rest) = self.sequence(variable, &record.steps);
        write!(output, "let decoded = {constructor} {{ {first_fields}").unwrap();
        for (name, expression) in fields {
            write!(output, "{name}: {expression}, ").unwrap();
        }
        writeln!(
            output,
            "span: {span} }};\n{rest}{variable}.finish()?;\nOk(decoded)"
        )
        .unwrap();
    }

    /// Sorts every node of the sequence `variable` into the local of the
    /// first field it fits, keeping the nodes that fit none in `errors`.
    fn any_order(&self, output: &mut String, variable: &str, steps: &[Step]) {
        let fields = steps.iter().filter_map(|step| match step {
            Step::Field(field) => Some(field),
            Step::Word(_) => None,
        });
        output.push_str("let mut errors = Vec::new();\n");
        for field in fields.clone() {
            let empty = match field.repetition.may_repeat() {
                true => "Vec::new()",
                false => "None",
            };
            writeln!(output, "let mut {} = {empty};", found(field)).unwrap();
        }
        writeln!(output, "for node in {variable}.rest() {{").unwrap();
        output.push_str("let mut attempts = support::Attempts::new(node);\n");
        for field in fields {
            let local = found(field);
            let keep = match field.repetition.may_repeat() {
                true => format!("{local}.push(value);"),
                false => format!(
                    "errors.extend(support::once(&mut {local}, value, node, {:?}));",
                    field.label
                ),
            };
            writeln!(
                output,
                "if let Some(value) = attempts.attempt({}) {{\n{keep}\ncontinue;\n}}",
                self.decoder(&field.value, field.boxed)
            )
            .unwrap();
        }
        output.push_str("errors.push(attempts.error());\n}\n");
    }

    /// The field expressions of an `ANY ORDER` record, from the locals
    /// [`Printer::any_order`] filled.
    fn any_order_fields(&self, variable: &str, steps: &[Step]) -> Vec<(String, String)> {
        steps
            .iter()
            .filter_map(|step| match step {
                Step::Field(field) => Some(field),
                Step::Word(_) => None,
            })
            .map(|field| {
                let local = found(field);
                let expected = &field.expected;
                let expression = match field.repetition {
                    Repetition::One => format!("{variable}.required({local}, {expected:?})?"),
                    Repetition::OneOrMore => {
                        format!("{variable}.at_least_one({local}, {expected:?})?")
                    }
                    _ => local,
                };
                (field.name.clone(), expression)
            })
            .collect()
    }

    fn print_decoder(&self, output: &mut String, definition: &Definition) {
        let index = self
            .definitions
            .iter()
            .position(|other| std::ptr::eq(other, definition))
            .expect("one of the definitions");
        let type_name = pascal_case(&definition.path);
        let returned = match &definition.body {
            Body::Variant { owner, .. } => self.type_name(*owner),
            Body::Transparent { .. } => self.value_type(&Value::Definition(index)),
            _ => type_name.clone(),
        };
        let (parameter, node) = match &definition.body {
            Body::Struct(Record {
                source: Source::Document,
                ..
            }) => ("nodes", format!("&[{}]", self.names.get("Node"))),
            _ => ("node", format!("&{}", self.names.get("Node"))),
        };
        writeln!(
            output,
            "{}fn {}({parameter}: {node}) -> {}<{returned}, {}> {{",
            match definition.is_rule {
                true => "pub ",
                false => "",
            },
            self.function_name(index),
            self.names.get("Result"),
            self.names.get("DecodeError"),
        )
        .unwrap();
        match &definition.body {
            Body::Alias(value) => writeln!(output, "{}", self.call(value)).unwrap(),
            Body::Struct(record) => self.print_record(output, record, &type_name),
            Body::Variant {
                owner,
                name,
                record,
            } => {
                let constructor = format!("{}::{name}", self.type_name(*owner));
                self.print_record(output, record, &constructor);
            }
            Body::Enum(variants) => {
                for (position, variant) in variants.iter().enumerate() {
                    let constructor = format!("{type_name}::{}", variant.name);
                    let choice = match &variant.payload {
                        Payload::Word(text) => {
                            format!("support::word(node, {text:?}).map({constructor})")
                        }
                        Payload::Value { value, boxed } => {
                            format!("{}.map({constructor})", self.boxed_call(value, *boxed))
                        }
                        Payload::Record(fields) => {
                            format!("{}(node)", self.function_name(*fields))
                        }
                    };
                    match position {
                        0 => writeln!(output, "{choice}"),
                        _ => writeln!(output, ".or_else(|error| error.or({choice}))"),
                    }
                    .unwrap();
                }
            }
            Body::Transparent { kind, steps } => {
                writeln!(output, "{}", Self::open_container(*kind)).unwrap();
                let (fields, rest) = self.sequence("elements", steps);
                match fields.first() {
                    Some((_, expression)) => writeln!(
                        output,
                        "let value = {expression};\n{rest}elements.finish()?;\nOk({} {{ value, span: node.span }})",
                        self.names.get("Spanned")
                    ),
                    None => writeln!(output, "{rest}elements.finish()?;\nOk(node.span)"),
                }
                .unwrap();
            }
        }
        output.push_str("}\n\n");
    }
}

/// The local an `ANY ORDER` decoder collects `field` in. The prefix keeps
/// it apart from the decoder's own locals, such as `node`.
fn found(field: &Field) -> String {
    format!("found_{}", field.name.trim_start_matches("r#"))
}

/// The value of a transparent bracket, if it has one.
fn only_field(steps: &[Step]) -> Option<&Field> {
    steps.iter().find_map(|step| match step {
        Step::Field(field) => Some(field),
        Step::Word(_) => None,
    })
}
