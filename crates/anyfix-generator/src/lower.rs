//! From the grammar as written to the checked [`Grammar`]: resolves rule
//! references, names fields and choices, and attaches each repetition to
//! the element it repeats. Everything wrong on the way is reported, and
//! lowering carries on to find the rest.

use anyfix_expr::{NamePart, NodeKind, Span};

use crate::diagnostic::Diagnostic;
use crate::grammar as ast;
use crate::grammar::support::number_value;
use crate::model::{
    Choice, ContainerKind, Element, Grammar, NumberPattern, Pattern, Repetition, Rule,
    StringCondition, WordKind, values,
};

/// Names that can't be turned into a Rust field, type or variant.
const UNUSABLE_NAMES: [&str; 3] = ["crate", "self", "super"];

pub fn lower(document: &ast::Document, source: &str) -> (Grammar, Vec<Diagnostic>) {
    let mut lowerer = Lowerer {
        source,
        rule_names: document.rule.iter().map(rule_name).collect(),
        diagnostics: Vec::new(),
    };
    lowerer.check_rule_names(document);
    let entry = match &document.entry.contents {
        ast::Contents::List(list) => &list.value[..],
        ast::Contents::Element(element) => std::slice::from_ref(element),
    };
    let entry = lowerer.sequence(entry, Names::Required);
    let rules = document
        .rule
        .iter()
        .map(|rule| Rule {
            name: rule_name(rule),
            name_span: rule.name.span,
            pattern: lowerer.pattern(&rule.arguments, Position::Rule),
        })
        .collect();
    (Grammar { entry, rules }, lowerer.diagnostics)
}

/// `key value _`, the call that defines a rule or names a field, is the
/// name `key value`.
fn rule_name(named: &ast::Named) -> String {
    let name = &named.name.value;
    name.strip_suffix(" _").unwrap_or(name).to_string()
}

struct Lowerer<'a> {
    source: &'a str,
    rule_names: Vec<String>,
    diagnostics: Vec<Diagnostic>,
}

/// Where a pattern stands, which decides whether a bracket becomes a struct.
#[derive(Clone, Copy, PartialEq)]
enum Position {
    /// The whole body of a rule.
    Rule,
    /// A field, a choice or an element.
    Inside,
}

#[derive(Clone, Copy, PartialEq)]
enum Names {
    /// Every element with a value becomes a field, so it needs a name.
    Required,
    /// The sequence has one value, which needs no name.
    Optional,
}

impl Lowerer<'_> {
    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(span, message));
    }

    /// Whether something inside `span` is already reported, in which case a
    /// second error about the same text would only repeat it.
    fn has_error_inside(&self, span: Span) -> bool {
        self.diagnostics
            .iter()
            .any(|earlier| span.start <= earlier.span.start && earlier.span.end <= span.end)
    }

    fn text(&self, span: Span) -> &str {
        &self.source[span.start..span.end]
    }

    fn check_rule_names(&mut self, document: &ast::Document) {
        for (index, rule) in document.rule.iter().enumerate() {
            let name = self.rule_names[index].clone();
            let span = rule.name.span;
            if self.rule_names[..index].contains(&name) {
                self.error(
                    span,
                    format!(
                        "found a second rule named `{name}`; expected each rule to be defined once; rename or remove one of them"
                    ),
                );
            } else if name == "document" {
                self.error(
                    span,
                    "found a rule named `document`; expected another name, since `Document` is the type of what `ENTRY` describes; rename the rule",
                );
            } else {
                self.check_usable(&name, span);
            }
        }
    }

    fn check_usable(&mut self, name: &str, span: Span) {
        if UNUSABLE_NAMES.contains(&name) {
            self.error(
                span,
                format!(
                    "found the name `{name}`; expected a name that can be a Rust identifier, which `{name}` can't; choose another name"
                ),
            );
        }
    }

    /// A pattern standing for exactly one node.
    fn pattern(&mut self, pattern: &ast::Pattern, position: Position) -> Pattern {
        match pattern {
            ast::Pattern::Reference(reference) => self.reference(reference),
            ast::Pattern::OneOf(one_of) => Pattern::OneOf(self.choices(&one_of.choices)),
            ast::Pattern::Repetition(repetition) => {
                self.error(
                    repetition.span(),
                    "found a repetition where exactly one node is matched; expected `ZERO OR MORE`, `ONE OR MORE`, `OPTIONAL` and `EACH` only on an element of `ENTRY`, of a bracket or of a `SPECIFIC ANYFIX CALL`, or after `ARGUMENTS:`; move it into a list, as in `SQUARE BRACKET [ZERO OR MORE: value]`",
                );
                self.pattern(repetition_of(pattern).1, position)
            }
            ast::Pattern::Container(container) => {
                let (kind, contents) = match container {
                    ast::Container::CurlyBrace { contents, .. } => {
                        (ContainerKind::CurlyBrace, contents)
                    }
                    ast::Container::SquareBracket { contents, .. } => {
                        (ContainerKind::SquareBracket, contents)
                    }
                    ast::Container::Parenthesis { contents, .. } => {
                        (ContainerKind::Parenthesis, contents)
                    }
                };
                let elements = match &**contents {
                    ast::Contents::List(list) => &list.value[..],
                    ast::Contents::Element(element) => std::slice::from_ref(element),
                };
                let names = match position {
                    Position::Rule => Names::Required,
                    Position::Inside => Names::Optional,
                };
                Pattern::Container {
                    kind,
                    elements: self.sequence(elements, names),
                }
            }
            ast::Pattern::SpecificCall(call) => self.specific_call(call),
            ast::Pattern::AnyCall(call) => self.any_call(call),
            ast::Pattern::Word(word) => {
                self.check_word(&word.text.value.value, word.text.span);
                Pattern::Word(word.text.value.value.clone())
            }
            ast::Pattern::AnyWord(any_word) => self.any_word(any_word),
            ast::Pattern::String(string) => Pattern::String(match string {
                ast::String::Any(_) => Vec::new(),
                ast::String::Exact { text, .. } => {
                    vec![StringCondition::Is(text.value.value.clone())]
                }
                ast::String::Conditions { conditions, .. } => self.string_conditions(conditions),
            }),
            ast::Pattern::Number(number) => Pattern::Number(self.number(number)),
        }
    }

    fn reference(&mut self, reference: &ast::Reference) -> Pattern {
        let name = &reference.name.value;
        if let Some(index) = self.rule_names.iter().position(|rule| rule == name) {
            return Pattern::Reference(index);
        }
        let fix = match closest(name, &self.rule_names) {
            Some(close) => format!("write `{close}`, or define `{name}: ...`"),
            None => format!("define `{name}: ...`"),
        };
        self.error(
            reference.span,
            format!(
                "found a reference to `{name}`, which is not a rule; expected the name of a rule of this grammar; {fix}"
            ),
        );
        Pattern::String(Vec::new())
    }

    fn choices(&mut self, choices: &[ast::Element]) -> Vec<Choice> {
        let mut lowered: Vec<Choice> = Vec::new();
        for choice in choices {
            let element = self.element(choice);
            if element.repetition != Repetition::One {
                self.error(
                    element.span,
                    "found a repetition as a choice of `ONE OF`; expected each choice to match exactly one node; repeat the whole `ONE OF` instead, as in `ZERO OR MORE: ONE OF [...]`",
                );
            }
            let name = match element.name.or_else(|| self.default_name(&element.pattern)) {
                Some(name) => name,
                None => {
                    let written = one_line(self.text(element.span));
                    if !self.has_error_inside(element.span) {
                        self.error(
                            element.span,
                            format!(
                                "found the choice `{written}` with no name; expected `name: pattern`, since only a rule reference or a `WORD` of letters names itself; add a name, as in `name: {written}`"
                            ),
                        );
                    }
                    continue;
                }
            };
            self.check_usable(&name, element.span);
            if lowered.iter().any(|earlier| earlier.name == name) {
                self.error(
                    element.span,
                    format!(
                        "found a second choice named `{name}`; expected each choice of a `ONE OF` to have its own name; name this one, as in `other {name}: ...`"
                    ),
                );
                continue;
            }
            lowered.push(Choice {
                name,
                pattern: element.pattern,
                span: element.span,
            });
        }
        lowered
    }

    /// The name a pattern has without being given one: a rule reference has
    /// its rule's, and `WORD "null"` is `null`.
    fn default_name(&self, pattern: &Pattern) -> Option<String> {
        match pattern {
            Pattern::Reference(rule) => Some(self.rule_names[*rule].clone()),
            Pattern::Word(text) => {
                let words: Vec<String> = text
                    .split('_')
                    .filter(|word| !word.is_empty())
                    .map(str::to_lowercase)
                    .collect();
                let is_name = !words.is_empty()
                    && words.iter().all(|word| {
                        word.starts_with(|c: char| c.is_ascii_lowercase())
                            && word
                                .chars()
                                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                    });
                is_name.then(|| words.join(" "))
            }
            _ => None,
        }
    }

    /// An element as written: its name if it has one, and its repetition.
    fn element(&mut self, element: &ast::Element) -> Element {
        let (name, pattern, span) = match element {
            ast::Element::Field(field) => (Some(rule_name(field)), &field.arguments, field.span),
            ast::Element::Pattern(pattern) => (None, pattern, pattern.span()),
        };
        let (repetition, pattern) = repetition_of(pattern);
        Element {
            name,
            repetition,
            pattern: self.pattern(pattern, Position::Inside),
            span,
        }
    }

    /// The elements of a bracket or of `ENTRY`, with their field names.
    fn sequence(&mut self, elements: &[ast::Element], names: Names) -> Vec<Element> {
        let mut elements: Vec<Element> = elements
            .iter()
            .map(|element| self.element(element))
            .collect();
        self.check_each_is_last(&elements);
        let names = match values(&elements).count() {
            0 | 1 => names,
            _ => Names::Required,
        };
        self.name_fields(&mut elements, names);
        elements
    }

    fn name_fields(&mut self, elements: &mut [Element], names: Names) {
        let mut taken: Vec<String> = Vec::new();
        for element in elements {
            if element.is_bare_word() {
                continue;
            }
            // The only value of an inline bracket stays unnamed: the bracket
            // then decodes to that value, with no struct around it.
            if element.name.is_none() && names == Names::Required {
                element.name = match &element.pattern {
                    Pattern::Reference(rule) => Some(self.rule_names[*rule].clone()),
                    _ => None,
                };
            }
            let Some(name) = &element.name else {
                if names == Names::Required && !self.has_error_inside(element.span) {
                    let written = one_line(self.text(element.span));
                    self.error(
                        element.span,
                        format!(
                            "found `{written}` with no name; expected `name: pattern`, since only a rule reference or a `WORD` can go unnamed here; add a name, as in `name: {written}`"
                        ),
                    );
                }
                continue;
            };
            let name = name.clone();
            if name == "span" {
                self.error(
                    element.span,
                    "found a field named `span`; expected another name, since every struct already has a `span`; rename the field",
                );
            }
            self.check_usable(&name, element.span);
            if taken.contains(&name) {
                self.error(
                    element.span,
                    format!(
                        "found a second field named `{name}`; expected each field to have its own name; name this one, as in `other {name}: ...`"
                    ),
                );
            }
            taken.push(name);
        }
    }

    /// `EACH` takes every element left, so nothing after it would ever match.
    fn check_each_is_last(&mut self, elements: &[Element]) {
        let after_each = elements
            .iter()
            .skip_while(|element| element.repetition != Repetition::Each)
            .nth(1);
        if let Some(after) = after_each {
            self.error(
                after.span,
                "found an element after `EACH`; expected `EACH` last, since it takes every element that is left; move it to the end, or use `ZERO OR MORE`, which stops at the first element that does not fit",
            );
        }
    }

    fn specific_call(&mut self, call: &ast::SpecificCall) -> Pattern {
        let mut parts: Vec<Element> = call.parts.iter().map(|part| self.element(part)).collect();
        self.check_each_is_last(&parts);
        for part in &parts {
            if matches!(part.pattern, Pattern::Word(_)) && part.repetition != Repetition::One {
                self.error(
                    part.span,
                    "found a repeated `WORD` among the parts of a call; expected each name word exactly once; write one `SPECIFIC ANYFIX CALL` per shape and join them with `ONE OF`",
                );
            }
        }
        self.name_fields(&mut parts, Names::Required);
        let has_word = parts
            .iter()
            .any(|part| matches!(part.pattern, Pattern::Word(_)));
        let most_items: usize = parts
            .iter()
            .map(|part| match part.repetition.may_repeat() {
                true => 2,
                false => 1,
            })
            .sum();
        if !has_word && most_items < 2 {
            self.error(
                call.span,
                "found a call of one slot and no word; expected a `WORD`, or at least two parts, since one item alone is never a call; write the part's pattern on its own",
            );
        }
        Pattern::SpecificCall(parts)
    }

    fn any_call(&mut self, call: &ast::AnyCall) -> Pattern {
        let mut name = None;
        let mut arguments = None;
        for condition in call.conditions.iter().flat_map(|list| list.iter()) {
            match condition {
                ast::CallCondition::Name { conditions, span } => {
                    if name.is_some() {
                        self.error(
                            *span,
                            "found a second `NAME`; expected one `NAME [...]` holding every condition on the name; merge the two lists",
                        );
                    }
                    name = Some(self.string_conditions(conditions));
                }
                ast::CallCondition::Arguments { pattern, span } => {
                    if arguments.is_some() {
                        self.error(
                            *span,
                            "found a second `ARGUMENTS`; expected one pattern for the arguments; remove one of them",
                        );
                    }
                    let (repetition, pattern) = repetition_of(pattern);
                    arguments = Some(Box::new(Element {
                        name: None,
                        repetition,
                        pattern: self.pattern(pattern, Position::Inside),
                        span: *span,
                    }));
                }
            }
        }
        Pattern::AnyCall {
            name: name.unwrap_or_default(),
            arguments,
        }
    }

    fn any_word(&mut self, any_word: &ast::AnyWord) -> Pattern {
        let mut kind = None;
        let mut conditions = Vec::new();
        for condition in any_word.conditions.iter().flat_map(|list| list.iter()) {
            let (this, span) = match condition {
                ast::WordCondition::Keyword(span) => (WordKind::Keyword, *span),
                ast::WordCondition::Name(span) => (WordKind::Name, *span),
                ast::WordCondition::Symbol(span) => (WordKind::Symbol, *span),
                ast::WordCondition::StringCondition(condition) => {
                    conditions.extend(self.string_condition(condition));
                    continue;
                }
            };
            if kind.is_some() {
                self.error(
                    span,
                    "found a second kind of word; expected at most one of `KEYWORD`, `NAME` and `SYMBOL`, since a word is only one of them; use `ONE OF` to allow several kinds",
                );
            }
            kind = Some(this);
        }
        Pattern::AnyWord { kind, conditions }
    }

    fn string_conditions(&mut self, conditions: &[ast::StringCondition]) -> Vec<StringCondition> {
        conditions
            .iter()
            .filter_map(|condition| self.string_condition(condition))
            .collect()
    }

    fn string_condition(&mut self, condition: &ast::StringCondition) -> Option<StringCondition> {
        Some(match condition {
            ast::StringCondition::StartsWith { text, .. } => {
                StringCondition::StartsWith(text.value.value.clone())
            }
            ast::StringCondition::EndsWith { text, .. } => {
                StringCondition::EndsWith(text.value.value.clone())
            }
            ast::StringCondition::Contains { text, .. } => {
                StringCondition::Contains(text.value.value.clone())
            }
            ast::StringCondition::MatchingRegex { regex, .. } => {
                if let Err(error) = regex::Regex::new(&regex.value.value) {
                    let reason = error.to_string();
                    let reason = reason.lines().last().unwrap_or("").trim();
                    self.error(
                        regex.span,
                        format!(
                            "found a regex that does not compile ({reason}); expected the syntax of Rust's `regex` crate; fix the regex, writing each backslash twice"
                        ),
                    );
                    return None;
                }
                StringCondition::MatchingRegex(regex.value.value.clone())
            }
        })
    }

    /// `WORD "x"` must hold what anyfix reads as one word.
    fn check_word(&mut self, text: &str, span: Span) {
        let nodes = anyfix_expr::parse(text).syntax.nodes();
        let is_one_word = matches!(
            &nodes[..],
            [node] if matches!(
                &node.kind,
                NodeKind::Call(call) if matches!(
                    &call.name[..],
                    [NamePart::Word { text: word, .. }] if word == text
                )
            )
        );
        if !is_one_word {
            self.error(
                span,
                format!(
                    "found `WORD {text:?}`, which is not one word; expected a single word, made of only name characters or only symbol characters; write one `WORD` for each word"
                ),
            );
        }
    }

    fn number(&mut self, number: &ast::Number) -> NumberPattern {
        let mut pattern = NumberPattern {
            integer: false,
            minimum: None,
            maximum: None,
        };
        match number {
            ast::Number::Any(_) => {}
            ast::Number::Exact { value, .. } => {
                let value = number_value(self.text(value.span));
                pattern = NumberPattern {
                    integer: matches!(value, crate::grammar::support::NumberValue::Integer(_)),
                    minimum: Some(value),
                    maximum: Some(value),
                };
            }
            ast::Number::Conditions { conditions, span } => {
                let mut seen = [false; 3];
                for condition in conditions.iter() {
                    let (index, keyword) = match condition {
                        ast::NumberCondition::Integer(_) => (0, "INTEGER"),
                        ast::NumberCondition::AtLeast { .. } => (1, "AT LEAST"),
                        ast::NumberCondition::AtMost { .. } => (2, "AT MOST"),
                    };
                    if std::mem::replace(&mut seen[index], true) {
                        self.error(
                            condition.span(),
                            format!(
                                "found a second `{keyword}`; expected each condition at most once; remove one of them"
                            ),
                        );
                    }
                    match condition {
                        ast::NumberCondition::Integer(_) => pattern.integer = true,
                        ast::NumberCondition::AtLeast { bound, .. } => {
                            pattern.minimum = Some(number_value(self.text(bound.span)));
                        }
                        ast::NumberCondition::AtMost { bound, .. } => {
                            pattern.maximum = Some(number_value(self.text(bound.span)));
                        }
                    }
                }
                if !pattern.is_satisfiable() {
                    self.error(
                        *span,
                        "found conditions that no number meets; expected `AT LEAST` to be no more than `AT MOST`, with an integer between them when `INTEGER` is given; fix the bounds",
                    );
                }
            }
        }
        pattern
    }
}

/// Source text on one line, shortened, for quoting in a message.
fn one_line(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match joined.chars().count() {
        0..=40 => joined,
        _ => format!("{}…", joined.chars().take(40).collect::<String>()),
    }
}

/// The rule name most like `name`, if one is close enough to be a typo.
fn closest<'a>(name: &str, rules: &'a [String]) -> Option<&'a str> {
    rules
        .iter()
        .map(|rule| (edit_distance(name, rule), rule.as_str()))
        .filter(|(distance, _)| *distance <= name.chars().count().div_ceil(3))
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, rule)| rule)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let substitution = previous + usize::from(x != *y);
            previous = row[j + 1];
            row[j + 1] = substitution.min(previous + 1).min(row[j] + 1);
        }
    }
    row[b.len()]
}

/// A pattern's repetition, and the pattern it repeats: `ZERO OR MORE: x` is
/// `x` zero or more times, and `x` alone is `x` once.
fn repetition_of(pattern: &ast::Pattern) -> (Repetition, &ast::Pattern) {
    match pattern {
        ast::Pattern::Repetition(repetition) => match repetition {
            ast::Repetition::ZeroOrMore { pattern, .. } => (Repetition::ZeroOrMore, pattern),
            ast::Repetition::OneOrMore { pattern, .. } => (Repetition::OneOrMore, pattern),
            ast::Repetition::Optional { pattern, .. } => (Repetition::Optional, pattern),
            ast::Repetition::Each { pattern, .. } => (Repetition::Each, pattern),
        },
        pattern => (Repetition::One, pattern),
    }
}
