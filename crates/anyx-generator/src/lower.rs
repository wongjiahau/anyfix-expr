//! From the grammar as written to the checked [`Grammar`]: resolves rule
//! references, names fields and choices, and attaches each repetition to
//! the element it repeats. Everything wrong on the way is reported, and
//! lowering carries on to find the rest.

use anyx::{NamePart, NodeKind, Span};

use crate::diagnostic::Diagnostic;
use crate::grammar as ast;
use crate::grammar::support::number_value;
use crate::model::{
    Choice, ContainerKind, Element, Grammar, LayoutRule, NumberPattern, Order, Pattern, Repetition,
    Rule, StringCondition, WordKind, values,
};

/// Names that can't be turned into a Rust field, type or variant.
const UNUSABLE_NAMES: [&str; 3] = ["crate", "self", "super"];

pub fn lower(document: &ast::Document, source: &str) -> (Grammar, Vec<Diagnostic>) {
    let mut lowerer = Lowerer {
        source,
        rule_names: document.rule.iter().map(rule_name).collect(),
        diagnostics: Vec::new(),
        layouts: Vec::new(),
    };
    lowerer.check_rule_names(document);
    let (entry, entry_order) = lowerer.contents(&document.entry.contents, Names::Required);
    let rules = document
        .rule
        .iter()
        .map(|rule| Rule {
            name: rule_name(rule),
            name_span: rule.name.span,
            pattern: lowerer.pattern(&rule.arguments, Position::Rule),
        })
        .collect();
    (
        Grammar {
            entry,
            entry_order,
            rules,
            layouts: lowerer.layouts,
        },
        lowerer.diagnostics,
    )
}

/// `key value _`, the call that defines a rule or names a field, is the
/// name `key value`.
fn rule_name(named: &ast::Named) -> String {
    let name = &named.name.text;
    name.strip_suffix(" _").unwrap_or(name).to_string()
}

struct Lowerer<'a> {
    source: &'a str,
    rule_names: Vec<String>,
    diagnostics: Vec<Diagnostic>,
    layouts: Vec<LayoutRule>,
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
                    "found a repetition where exactly one node is matched; expected `ZERO OR MORE`, `ONE OR MORE`, `OPTIONAL` and `EACH` only on an element of `ENTRY`, of a bracket or of an `ANYFIX CALL [...]`, or after `ARGUMENTS:`; move it into a list, as in `SQUARE BRACKET [ZERO OR MORE: value]`",
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
                let names = match position {
                    Position::Rule => Names::Required,
                    Position::Inside => Names::Optional,
                };
                let (elements, order) = self.contents(contents, names);
                Pattern::Container {
                    kind,
                    elements,
                    order,
                }
            }
            ast::Pattern::SpecificCall(call) => self.specific_call(call),
            ast::Pattern::AnyCall(call) => self.any_call(call),
            ast::Pattern::Word(word) => {
                self.check_word(&word.text.value.value, word.text.span);
                Pattern::Word(word.text.value.value.clone())
            }
            ast::Pattern::AnyWord(any_word) => self.any_word(any_word),
            ast::Pattern::ExactWords(exact) => {
                let words = self.exact_words(exact);
                match words.contains(' ') {
                    false => Pattern::Word(words),
                    // A call of those name words and no arguments.
                    true => Pattern::SpecificCall(vec![Element {
                        name: None,
                        repetition: Repetition::One,
                        pattern: Pattern::Word(words),
                        span: exact.span,
                    }]),
                }
            }
            ast::Pattern::Words(words) => {
                self.error(
                    words.span,
                    "found `WORDS` outside the parts of a call; expected it only among the parts of an `ANYFIX CALL [...]`, since only a call has name words; put it in one, as in `ANYFIX CALL [name: WORDS, value]`",
                );
                self.words(words)
            }
            ast::Pattern::String(string) => Pattern::String(match string {
                ast::String::Any(_) => Vec::new(),
                ast::String::Exact { text, .. } => {
                    vec![StringCondition::Is(text.value.value.clone())]
                }
                ast::String::Conditions { conditions, .. } => self.string_conditions(conditions),
            }),
            ast::Pattern::Number(number) => Pattern::Number(self.number(number)),
            ast::Pattern::AnyNode(_) => Pattern::AnyNode,
        }
    }

    fn reference(&mut self, reference: &ast::Reference) -> Pattern {
        let name = &reference.name.text;
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
                    .split(['_', ' '])
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
            // `WORDS "sans serif"` is `sans serif`.
            Pattern::SpecificCall(parts) => match &parts[..] {
                [only] if only.is_bare_word() => self.default_name(&only.pattern),
                _ => None,
            },
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

    /// What a bracket or `ENTRY` holds, and the order it is matched in.
    fn contents(&mut self, contents: &ast::Contents, names: Names) -> (Vec<Element>, Order) {
        match contents {
            ast::Contents::List(list) => (self.sequence(&list.value, names), Order::Written),
            ast::Contents::Element(element) => (
                self.sequence(std::slice::from_ref(element), names),
                Order::Written,
            ),
            ast::Contents::AnyOrder(any_order) => {
                (self.any_order(&any_order.elements.value), Order::Any)
            }
        }
    }

    /// The elements of `ANY ORDER [...]`: each a named field, since every
    /// one is looked for wherever it stands.
    fn any_order(&mut self, elements: &[ast::Element]) -> Vec<Element> {
        let mut elements: Vec<Element> = elements
            .iter()
            .map(|element| self.element(element))
            .collect();
        for element in &elements {
            if element.is_bare_word() {
                self.error(
                    element.span,
                    "found an unnamed `WORD` in `ANY ORDER`; expected every element to be a field, since a word that is only matched has no place when the order is free; name it, as in `flag: WORD \"x\"`",
                );
            }
            if element.repetition == Repetition::Each {
                self.error(
                    element.span,
                    "found `EACH` in `ANY ORDER`; expected `OPTIONAL`, `ZERO OR MORE`, `ONE OR MORE` or no repetition, since a node that fits no element is already kept in `errors` and the rest still decode; use `ZERO OR MORE`",
                );
            }
        }
        self.name_fields(&mut elements, Names::Required);
        for element in &elements {
            if element.name.as_deref() == Some("errors") {
                self.error(
                    element.span,
                    "found a field named `errors` in `ANY ORDER`; expected another name, since `errors` holds the nodes that fit no element; rename the field",
                );
            }
        }
        elements
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
        let (call_parts, span) = match call {
            ast::SpecificCall::Formatted { parts, span, .. }
            | ast::SpecificCall::Plain { parts, span } => (parts, *span),
        };
        let mut parts: Vec<Element> = call_parts
            .iter()
            .flat_map(|part| self.call_part(part))
            .collect();
        self.check_each_is_last(&parts);
        for part in &parts {
            let what = match part.pattern {
                Pattern::Word(_) => "WORD",
                Pattern::Words(_) => "WORDS",
                _ => continue,
            };
            if part.repetition != Repetition::One {
                self.error(
                    part.span,
                    format!(
                        "found a repeated `{what}` among the parts of a call; expected each name word exactly once; write one `ANYFIX CALL` per shape and join them with `ONE OF`"
                    ),
                );
            }
        }
        for pair in parts.windows(2) {
            let [before, after] = pair else { continue };
            let is_words = |part: &Element| matches!(part.pattern, Pattern::Words(_));
            let is_word = |part: &Element| matches!(part.pattern, Pattern::Word(_));
            if is_words(before) && (is_word(after) || is_words(after))
                || is_word(before) && is_words(after)
            {
                self.error(
                    after.span,
                    "found a name word right next to `WORDS`; expected a slot between them, since `WORDS` takes every name word in a row; put a slot between them, or write the words as one `WORDS`",
                );
            }
        }
        self.name_fields(&mut parts, Names::Required);
        let has_word = parts
            .iter()
            .any(|part| matches!(part.pattern, Pattern::Word(_) | Pattern::Words(_)));
        let most_items: usize = parts
            .iter()
            .map(|part| match part.repetition.may_repeat() {
                true => 2,
                false => 1,
            })
            .sum();
        if !has_word && most_items < 2 {
            self.error(
                span,
                "found a call of one slot and no word; expected a `WORD`, or at least two parts, since one item alone is never a call; write the part's pattern on its own",
            );
        }
        if let ast::SpecificCall::Formatted { format, .. } = call {
            self.layout(&parts, format);
        }
        Pattern::SpecificCall(parts)
    }

    /// Checks a call's `FORMAT` against its parts and keeps it.
    fn layout(&mut self, parts: &[Element], format: &[ast::FormatRule]) {
        let words: Vec<String> = parts
            .iter()
            .filter_map(|part| match &part.pattern {
                Pattern::Word(text) => Some(text.split(' ').map(str::to_string).collect::<Vec<_>>()),
                _ => None,
            })
            .flatten()
            .collect();
        let mut break_before = Vec::new();
        for rule in format {
            let text = rule.text.value.value.clone();
            let span = rule.text.span;
            let position = parts
                .iter()
                .position(|part| matches!(&part.pattern, Pattern::Word(word) if *word == text));
            match position {
                None => self.error(
                    span,
                    format!(
                        "found `BREAK BEFORE WORD {text:?}`, which is not a word of the call; expected one of its `WORD` parts{}",
                        match words.is_empty() {
                            true => String::new(),
                            false => format!(
                                ": {}",
                                words.iter().map(|word| format!("{word:?}")).collect::<Vec<_>>().join(", ")
                            ),
                        }
                    ),
                ),
                Some(0) => self.error(
                    span,
                    format!(
                        "found `BREAK BEFORE WORD {text:?}`, the call's first part; expected a later word, since a call can't break before it starts"
                    ),
                ),
                Some(_) if break_before.contains(&text) => self.error(
                    span,
                    format!("found `BREAK BEFORE WORD {text:?}` twice; expected each word once"),
                ),
                Some(_) => break_before.push(text),
            }
        }
        self.layouts.push(LayoutRule {
            words,
            break_before,
        });
    }

    /// One of the parts of an `ANYFIX CALL [...]`, where `WORDS` may stand.
    /// `WORDS "a b"` is one part for each of its words.
    fn call_part(&mut self, part: &ast::Element) -> Vec<Element> {
        let (name, pattern, span) = match part {
            ast::Element::Field(field) => (Some(rule_name(field)), &field.arguments, field.span),
            ast::Element::Pattern(pattern) => (None, pattern, pattern.span()),
        };
        let (repetition, inner) = repetition_of(pattern);
        match inner {
            ast::Pattern::Words(words) => vec![Element {
                name,
                repetition,
                pattern: self.words(words),
                span,
            }],
            // One part matching the words in a row; named, it keeps their span.
            ast::Pattern::ExactWords(exact) => vec![Element {
                name,
                repetition,
                pattern: Pattern::Word(self.exact_words(exact)),
                span,
            }],
            _ => vec![self.element(part)],
        }
    }

    /// The words of `WORDS "a b"`, separated by one space each: the text of
    /// a `WORD` that matches them in a row.
    fn exact_words(&mut self, exact: &ast::ExactWords) -> String {
        let text = &exact.text.value.value;
        let span = exact.text.span;
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            self.error(
                span,
                "found `WORDS` of no words; expected one or more words separated by spaces, as in `WORDS \"max size\"`",
            );
        }
        for word in &words {
            if !is_one_word(word) {
                self.error(
                    span,
                    format!(
                        "found `{word}` in `WORDS {text:?}`, which is not a word; expected words made of only name characters or only symbol characters, separated by spaces"
                    ),
                );
            }
        }
        words.join(" ")
    }

    fn words(&mut self, words: &ast::Words) -> Pattern {
        let conditions = words.conditions.iter().flat_map(|list| list.iter());
        Pattern::Words(
            conditions
                .filter_map(|condition| self.name_condition(condition))
                .collect(),
        )
    }

    fn name_conditions(&mut self, conditions: &[ast::NameCondition]) -> Vec<StringCondition> {
        conditions
            .iter()
            .filter_map(|condition| self.name_condition(condition))
            .collect()
    }

    fn name_condition(&mut self, condition: &ast::NameCondition) -> Option<StringCondition> {
        match condition {
            ast::NameCondition::NoKeyword { .. } => Some(StringCondition::NoKeyword),
            ast::NameCondition::StringCondition(condition) => self.string_condition(condition),
        }
    }

    fn any_call(&mut self, call: &ast::AnyCall) -> Pattern {
        let mut name = None;
        let mut arguments = None;
        let mut no_arguments = None;
        for condition in call.conditions.iter().flat_map(|list| list.iter()) {
            match condition {
                ast::CallCondition::Name { conditions, span } => {
                    if name.is_some() {
                        self.error(
                            *span,
                            "found a second `NAME`; expected one `NAME [...]` holding every condition on the name; merge the two lists",
                        );
                    }
                    name = Some(self.name_conditions(conditions));
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
                ast::CallCondition::NoArguments { span } => no_arguments = Some(*span),
            }
        }
        let mut name = name.unwrap_or_default();
        if let Some(span) = no_arguments {
            if arguments.is_some() {
                self.error(
                    span,
                    "found both `ARGUMENTS` and `NO ARGUMENTS`; expected at most one of them; remove one",
                );
            }
            // A call has an argument for each `_` in its name.
            name.push(StringCondition::NoSlots);
        }
        Pattern::AnyCall { name, arguments }
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

    /// `WORD "x"` must hold what anyx reads as one word.
    fn check_word(&mut self, text: &str, span: Span) {
        if !is_one_word(text) {
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
            signed: false,
            minimum: None,
            maximum: None,
        };
        match number {
            ast::Number::Any(_) => {}
            ast::Number::Exact { value, .. } => {
                let value = number_value(self.text(value.span));
                pattern = NumberPattern {
                    integer: matches!(value, crate::grammar::support::NumberValue::Integer(_)),
                    signed: false,
                    minimum: Some(value),
                    maximum: Some(value),
                };
            }
            ast::Number::Conditions { conditions, span } => {
                let mut seen = [false; 4];
                for condition in conditions.iter() {
                    let (index, keyword) = match condition {
                        ast::NumberCondition::Integer(_) => (0, "INTEGER"),
                        ast::NumberCondition::Signed(_) => (3, "SIGNED"),
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
                        ast::NumberCondition::Signed(_) => pattern.signed = true,
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

/// Whether anyx reads `text` as exactly one name word or symbol word.
fn is_one_word(text: &str) -> bool {
    let nodes = anyx::parse(text).syntax.nodes();
    matches!(
        &nodes[..],
        [node] if matches!(
            &node.kind,
            NodeKind::Call(call) if matches!(
                &call.name[..],
                [NamePart::Word { text: word, .. }] if word == text
            )
        )
    )
}
