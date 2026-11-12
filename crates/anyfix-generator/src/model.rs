//! A checked grammar: every reference resolved, every field and choice
//! named, and every repetition attached to the element it repeats.

use anyfix_expr::Span;

use crate::grammar::support::NumberValue;

#[derive(Clone, Debug, PartialEq)]
pub struct Grammar {
    /// The document's top-level sequence.
    pub entry: Vec<Element>,
    pub entry_order: Order,
    pub rules: Vec<Rule>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    /// The name as written, such as `key value`.
    pub name: String,
    pub name_span: Span,
    pub pattern: Pattern,
}

/// A pattern that matches exactly one node.
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    /// The index of the rule in [`Grammar::rules`].
    Reference(usize),
    OneOf(Vec<Choice>),
    Container {
        kind: ContainerKind,
        elements: Vec<Element>,
        order: Order,
    },
    SpecificCall(Vec<Element>),
    AnyCall {
        name: Vec<StringCondition>,
        /// Without it, the arguments stay raw nodes.
        arguments: Option<Box<Element>>,
    },
    Word(String),
    AnyWord {
        kind: Option<WordKind>,
        conditions: Vec<StringCondition>,
    },
    String(Vec<StringCondition>),
    Number(NumberPattern),
    /// Any one node, decoded to itself.
    AnyNode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub name: String,
    pub pattern: Pattern,
    pub span: Span,
}

/// One step of a sequence: the elements of a bracket or the document, or
/// the parts of a call.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// The field this element fills. An unnamed `WORD` fills none, and
    /// neither does the only element of a bracket that needs no name.
    /// Among the parts of an `ANYFIX CALL [...]`, a `WORD` is always one
    /// of the call's name words, named or not.
    pub name: Option<String>,
    pub repetition: Repetition,
    pub pattern: Pattern,
    pub span: Span,
}

impl Element {
    /// A `WORD` that is only matched: among the parts of a call it is a name
    /// word, and it has no value worth keeping.
    pub fn is_bare_word(&self) -> bool {
        self.name.is_none()
            && self.repetition == Repetition::One
            && matches!(self.pattern, Pattern::Word(_))
    }
}

/// The elements of a sequence that have a value.
pub fn values(elements: &[Element]) -> impl Iterator<Item = &Element> {
    elements.iter().filter(|element| !element.is_bare_word())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repetition {
    One,
    Optional,
    ZeroOrMore,
    OneOrMore,
    /// Every element left, each decoded on its own, so that one that does
    /// not fit is kept as its error and the rest still decode.
    Each,
}

impl Repetition {
    /// Whether no node at all satisfies it.
    pub fn may_be_empty(self) -> bool {
        matches!(
            self,
            Repetition::Optional | Repetition::ZeroOrMore | Repetition::Each
        )
    }

    /// Whether it takes more than one node.
    pub fn may_repeat(self) -> bool {
        matches!(
            self,
            Repetition::ZeroOrMore | Repetition::OneOrMore | Repetition::Each
        )
    }
}

/// How the elements of a bracket or of the document are matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// One after another, as listed.
    Written,
    /// `ANY ORDER`: each node goes to the first element it fits, wherever
    /// it stands. A node that fits none, or a second node for an element
    /// that is not repeated, is kept as an error and the rest still decode.
    Any,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerKind {
    CurlyBrace,
    SquareBracket,
    Parenthesis,
}

impl ContainerKind {
    /// How the bracket looks in an error message.
    pub fn picture(self) -> &'static str {
        match self {
            ContainerKind::CurlyBrace => "`{...}`",
            ContainerKind::SquareBracket => "`[...]`",
            ContainerKind::Parenthesis => "`(...)`",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordKind {
    Keyword,
    Name,
    Symbol,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StringCondition {
    /// `STRING "x"`: the whole value.
    Is(String),
    StartsWith(String),
    EndsWith(String),
    Contains(String),
    MatchingRegex(String),
    /// No word of a call's name is a `_` slot: `NO ARGUMENTS`.
    NoSlots,
}

impl StringCondition {
    pub fn holds(&self, text: &str) -> bool {
        match self {
            StringCondition::Is(expected) => text == expected,
            StringCondition::StartsWith(prefix) => text.starts_with(prefix.as_str()),
            StringCondition::EndsWith(suffix) => text.ends_with(suffix.as_str()),
            StringCondition::Contains(part) => text.contains(part.as_str()),
            StringCondition::MatchingRegex(regex) => {
                regex::Regex::new(regex).is_ok_and(|regex| regex.is_match(text))
            }
            StringCondition::NoSlots => !text.split(' ').any(|word| word == "_"),
        }
    }
}

/// The numbers a `NUMBER` pattern matches: those from `minimum` to
/// `maximum`, and only whole ones when `integer`. When `signed`, a sign
/// written apart from the number, as in `+ 4` or `- 4`, is taken too.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberPattern {
    pub integer: bool,
    pub signed: bool,
    pub minimum: Option<NumberValue>,
    pub maximum: Option<NumberValue>,
}

/// The Rust type a number pattern decodes to, with the range to check.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumberType {
    Integer {
        rust: &'static str,
        minimum: i128,
        maximum: i128,
    },
    Float {
        minimum: f64,
        maximum: f64,
    },
}

impl NumberPattern {
    /// An integer with both bounds gets the smallest type that holds them,
    /// one with a bound missing gets `i64` (or `u64` when it can't be
    /// negative), and anything else is an `f64`.
    pub fn number_type(&self) -> NumberType {
        if !self.integer {
            return NumberType::Float {
                minimum: self
                    .minimum
                    .map_or(f64::NEG_INFINITY, NumberValue::as_float),
                maximum: self.maximum.map_or(f64::INFINITY, NumberValue::as_float),
            };
        }
        let lowest = self.minimum.map(|bound| match bound {
            NumberValue::Integer(value) => value,
            NumberValue::Float(value) => value.ceil() as i128,
        });
        let highest = self.maximum.map(|bound| match bound {
            NumberValue::Integer(value) => value,
            NumberValue::Float(value) => value.floor() as i128,
        });
        const UNSIGNED: [(&str, i128); 4] = [
            ("u8", u8::MAX as i128),
            ("u16", u16::MAX as i128),
            ("u32", u32::MAX as i128),
            ("u64", u64::MAX as i128),
        ];
        const SIGNED: [(&str, i128, i128); 4] = [
            ("i8", i8::MIN as i128, i8::MAX as i128),
            ("i16", i16::MIN as i128, i16::MAX as i128),
            ("i32", i32::MIN as i128, i32::MAX as i128),
            ("i64", i64::MIN as i128, i64::MAX as i128),
        ];
        let (rust, minimum, maximum) = match (lowest, highest) {
            (Some(lowest), Some(highest)) => {
                let rust = if lowest >= 0 {
                    UNSIGNED
                        .iter()
                        .find(|(_, maximum)| highest <= *maximum)
                        .map(|(rust, _)| *rust)
                } else {
                    SIGNED
                        .iter()
                        .find(|(_, minimum, maximum)| *minimum <= lowest && highest <= *maximum)
                        .map(|(rust, _, _)| *rust)
                };
                (rust.unwrap_or("i128"), lowest, highest)
            }
            (Some(lowest), None) if lowest >= 0 => ("u64", lowest, u64::MAX as i128),
            (Some(lowest), None) if lowest >= i64::MIN as i128 => ("i64", lowest, i64::MAX as i128),
            (Some(lowest), None) => ("i128", lowest, i128::MAX),
            (None, Some(highest)) if highest <= i64::MAX as i128 => {
                ("i64", i64::MIN as i128, highest)
            }
            (None, Some(highest)) => ("i128", i128::MIN, highest),
            (None, None) => ("i64", i64::MIN as i128, i64::MAX as i128),
        };
        NumberType::Integer {
            rust,
            minimum,
            maximum,
        }
    }

    /// Whether any number at all fits.
    pub fn is_satisfiable(&self) -> bool {
        match self.number_type() {
            NumberType::Integer {
                minimum, maximum, ..
            } => minimum <= maximum,
            NumberType::Float { minimum, maximum } => minimum <= maximum,
        }
    }
}

/// `a` or `an`, then the name: `an expression`, `a key value`. A plural
/// name, such as `contents`, stands as it is.
pub fn with_article(name: &str) -> String {
    if name.ends_with('s') && !name.ends_with("ss") {
        return name.to_string();
    }
    let article = match name.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    };
    format!("{article} {name}")
}

fn number_text(value: NumberValue) -> String {
    match value {
        NumberValue::Integer(value) => value.to_string(),
        NumberValue::Float(value) => value.to_string(),
    }
}

/// `x`, `x and y`, `x, y and z`.
fn join_and(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn conditions_text(conditions: &[StringCondition]) -> String {
    let parts: Vec<String> = conditions
        .iter()
        .map(|condition| match condition {
            StringCondition::Is(text) => format!("is {text:?}"),
            StringCondition::StartsWith(text) => format!("starts with {text:?}"),
            StringCondition::EndsWith(text) => format!("ends with {text:?}"),
            StringCondition::Contains(text) => format!("contains {text:?}"),
            StringCondition::MatchingRegex(regex) => format!("matches the regex `{regex}`"),
            StringCondition::NoSlots => "has no `_`".to_string(),
        })
        .collect();
    join_and(&parts)
}

impl Grammar {
    /// How an error message names what `pattern` expects.
    pub fn describe(&self, pattern: &Pattern) -> Vec<String> {
        let one = match pattern {
            Pattern::Reference(rule) => with_article(&self.rules[*rule].name),
            Pattern::OneOf(choices) => {
                let mut all = Vec::new();
                for choice in choices {
                    for one in self.describe(&choice.pattern) {
                        if !all.contains(&one) {
                            all.push(one);
                        }
                    }
                }
                return all;
            }
            Pattern::Container { kind, .. } => kind.picture().to_string(),
            Pattern::SpecificCall(parts) => format!("the call `{}`", call_picture(parts)),
            Pattern::AnyCall { name, .. } if name.is_empty() => "a call".to_string(),
            Pattern::AnyCall { name, .. } if name[..] == [StringCondition::NoSlots] => {
                "a call with no arguments".to_string()
            }
            Pattern::AnyCall { name, .. } => {
                format!("a call whose name {}", conditions_text(name))
            }
            Pattern::Word(text) => format!("the word `{text}`"),
            Pattern::AnyWord { kind, conditions } => {
                let kind = match kind {
                    None => "a word",
                    Some(WordKind::Keyword) => "a keyword",
                    Some(WordKind::Name) => "a name",
                    Some(WordKind::Symbol) => "a symbol",
                };
                match conditions.is_empty() {
                    true => kind.to_string(),
                    false => format!("{kind} that {}", conditions_text(conditions)),
                }
            }
            Pattern::String(conditions) => match &conditions[..] {
                [] => "a string".to_string(),
                [StringCondition::Is(text)] => format!("the string {text:?}"),
                _ => format!("a string that {}", conditions_text(conditions)),
            },
            Pattern::Number(number) => describe_number(number),
            Pattern::AnyNode => "a node".to_string(),
        };
        vec![one]
    }

    /// The description as one phrase: `a string or the word `null``.
    pub fn describe_one(&self, pattern: &Pattern) -> String {
        match &self.describe(pattern)[..] {
            [] => "nothing".to_string(),
            [only] => only.clone(),
            [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
        }
    }
}

fn describe_number(number: &NumberPattern) -> String {
    let noun = if number.integer {
        "an integer"
    } else {
        "a number"
    };
    match (number.minimum, number.maximum) {
        (Some(minimum), Some(maximum)) if minimum == maximum => {
            format!("the number {}", number_text(minimum))
        }
        (Some(minimum), Some(maximum)) => format!(
            "{noun} from {} to {}",
            number_text(minimum),
            number_text(maximum)
        ),
        (Some(minimum), None) => format!("{noun} of at least {}", number_text(minimum)),
        (None, Some(maximum)) => format!("{noun} of at most {}", number_text(maximum)),
        (None, None) => noun.to_string(),
    }
}

/// The name a call must have: its words, and `_` for each slot, marked
/// `_?`, `_*` or `_+` when the slot repeats.
pub fn call_picture(parts: &[Element]) -> String {
    parts
        .iter()
        .map(|part| match (&part.pattern, part.repetition) {
            (Pattern::Word(text), _) => text.clone(),
            (_, Repetition::One) => "_".to_string(),
            (_, Repetition::Optional) => "_?".to_string(),
            (_, Repetition::ZeroOrMore | Repetition::Each) => "_*".to_string(),
            (_, Repetition::OneOrMore) => "_+".to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}
