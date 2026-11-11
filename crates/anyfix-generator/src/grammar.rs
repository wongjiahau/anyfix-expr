//! The grammar language as a typed tree, with its decoder. Written by hand
//! in the shape the generator emits, until the generator can write it from
//! `grammar.anyfix`.

#[path = "support.rs"]
pub mod support;

use anyfix_expr::{Node, Span, StringLiteral};
pub use support::{DecodeError, Spanned};

static REGEX_1: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new("^[a-z][a-z0-9]*( [a-z][a-z0-9]*)* _$").expect("the generator checked it")
});

static REGEX_2: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new("^[a-z][a-z0-9]*( [a-z][a-z0-9]*)*$").expect("the generator checked it")
});

/// `ENTRY [entry, ZERO OR MORE: rule]`
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub entry: Entry,
    pub rule: Vec<Rule>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub contents: Contents,
    pub span: Span,
}

pub type Rule = Named;

#[derive(Clone, Debug, PartialEq)]
pub struct Named {
    pub name: Spanned<std::string::String>,
    pub arguments: Pattern,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Contents {
    List(Spanned<Vec<Element>>),
    Element(Element),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Element {
    Field(Named),
    Pattern(Pattern),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Reference(Reference),
    OneOf(OneOf),
    Repetition(Repetition),
    Container(Container),
    SpecificCall(SpecificCall),
    AnyCall(AnyCall),
    Word(Word),
    AnyWord(AnyWord),
    String(String),
    Number(Number),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    pub name: Spanned<std::string::String>,
    pub arguments: Vec<Node>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OneOf {
    pub choices: Spanned<Vec<Element>>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Repetition {
    ZeroOrMore { pattern: Box<Pattern>, span: Span },
    OneOrMore { pattern: Box<Pattern>, span: Span },
    Optional { pattern: Box<Pattern>, span: Span },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Container {
    CurlyBrace { contents: Box<Contents>, span: Span },
    SquareBracket { contents: Box<Contents>, span: Span },
    Parenthesis { contents: Box<Contents>, span: Span },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpecificCall {
    pub parts: Spanned<Vec<Element>>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnyCall {
    pub conditions: Option<Spanned<Vec<CallCondition>>>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CallCondition {
    Name {
        conditions: Spanned<Vec<StringCondition>>,
        span: Span,
    },
    Arguments {
        pattern: Pattern,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: Spanned<StringLiteral>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnyWord {
    pub conditions: Option<Spanned<Vec<WordCondition>>>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WordCondition {
    Keyword(Span),
    Name(Span),
    Symbol(Span),
    StringCondition(StringCondition),
}

#[derive(Clone, Debug, PartialEq)]
pub enum String {
    Any(Span),
    Exact {
        text: Spanned<StringLiteral>,
        span: Span,
    },
    Conditions {
        conditions: Spanned<Vec<StringCondition>>,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum StringCondition {
    StartsWith {
        text: Spanned<StringLiteral>,
        span: Span,
    },
    EndsWith {
        text: Spanned<StringLiteral>,
        span: Span,
    },
    Contains {
        text: Spanned<StringLiteral>,
        span: Span,
    },
    MatchingRegex {
        regex: Spanned<StringLiteral>,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Number {
    Any(Span),
    Exact {
        value: Spanned<f64>,
        span: Span,
    },
    Conditions {
        conditions: Spanned<Vec<NumberCondition>>,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum NumberCondition {
    Integer(Span),
    AtLeast { bound: Spanned<f64>, span: Span },
    AtMost { bound: Spanned<f64>, span: Span },
}

impl Contents {
    pub fn span(&self) -> Span {
        match self {
            Contents::List(value) => value.span,
            Contents::Element(value) => value.span(),
        }
    }
}

impl Element {
    pub fn span(&self) -> Span {
        match self {
            Element::Field(value) => value.span,
            Element::Pattern(value) => value.span(),
        }
    }
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Reference(value) => value.span,
            Pattern::OneOf(value) => value.span,
            Pattern::Repetition(value) => value.span(),
            Pattern::Container(value) => value.span(),
            Pattern::SpecificCall(value) => value.span,
            Pattern::AnyCall(value) => value.span,
            Pattern::Word(value) => value.span,
            Pattern::AnyWord(value) => value.span,
            Pattern::String(value) => value.span(),
            Pattern::Number(value) => value.span(),
        }
    }
}

impl Repetition {
    pub fn span(&self) -> Span {
        match self {
            Repetition::ZeroOrMore { span, .. }
            | Repetition::OneOrMore { span, .. }
            | Repetition::Optional { span, .. } => *span,
        }
    }
}

impl Container {
    pub fn span(&self) -> Span {
        match self {
            Container::CurlyBrace { span, .. }
            | Container::SquareBracket { span, .. }
            | Container::Parenthesis { span, .. } => *span,
        }
    }
}

impl CallCondition {
    pub fn span(&self) -> Span {
        match self {
            CallCondition::Name { span, .. } | CallCondition::Arguments { span, .. } => *span,
        }
    }
}

impl WordCondition {
    pub fn span(&self) -> Span {
        match self {
            WordCondition::Keyword(span)
            | WordCondition::Name(span)
            | WordCondition::Symbol(span) => *span,
            WordCondition::StringCondition(value) => value.span(),
        }
    }
}

impl String {
    pub fn span(&self) -> Span {
        match self {
            String::Any(span) | String::Exact { span, .. } | String::Conditions { span, .. } => {
                *span
            }
        }
    }
}

impl StringCondition {
    pub fn span(&self) -> Span {
        match self {
            StringCondition::StartsWith { span, .. }
            | StringCondition::EndsWith { span, .. }
            | StringCondition::Contains { span, .. }
            | StringCondition::MatchingRegex { span, .. } => *span,
        }
    }
}

impl Number {
    pub fn span(&self) -> Span {
        match self {
            Number::Any(span) | Number::Exact { span, .. } | Number::Conditions { span, .. } => {
                *span
            }
        }
    }
}

impl NumberCondition {
    pub fn span(&self) -> Span {
        match self {
            NumberCondition::Integer(span)
            | NumberCondition::AtLeast { span, .. }
            | NumberCondition::AtMost { span, .. } => *span,
        }
    }
}

/// `ENTRY [entry, ZERO OR MORE: rule]`
pub fn decode_document(nodes: &[Node]) -> Result<Document, DecodeError> {
    let mut elements = support::document(nodes);
    let decoded = Document {
        entry: elements.one(decode_entry, "an entry")?,
        rule: elements.zero_or_more(decode_rule),
        span: support::document_span(nodes),
    };
    elements.finish()?;
    Ok(decoded)
}

pub fn decode_entry(node: &Node) -> Result<Entry, DecodeError> {
    let mut parts = support::call(node, "the call `ENTRY _`")?;
    let decoded = Entry {
        contents: parts.word("ENTRY")?.one(decode_contents, "contents")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

pub fn decode_rule(node: &Node) -> Result<Rule, DecodeError> {
    decode_named(node)
}

pub fn decode_named(node: &Node) -> Result<Named, DecodeError> {
    let (name, arguments) = support::any_call(
        node,
        |name| REGEX_1.is_match(name),
        "a call whose name matches the regex `^[a-z][a-z0-9]*( [a-z][a-z0-9]*)* _$`",
    )?;
    let mut arguments = support::arguments(node, arguments);
    let decoded = Named {
        name,
        arguments: arguments.one(decode_pattern, "a pattern")?,
        span: node.span,
    };
    arguments.finish()?;
    Ok(decoded)
}

pub fn decode_contents(node: &Node) -> Result<Contents, DecodeError> {
    decode_contents_list(node)
        .map(Contents::List)
        .or_else(|error| error.or(decode_element(node).map(Contents::Element)))
}

fn decode_contents_list(node: &Node) -> Result<Spanned<Vec<Element>>, DecodeError> {
    let mut elements = support::square_bracket(node)?;
    let value = elements.zero_or_more(decode_element);
    elements.finish()?;
    Ok(Spanned {
        value,
        span: node.span,
    })
}

pub fn decode_element(node: &Node) -> Result<Element, DecodeError> {
    decode_named(node)
        .map(Element::Field)
        .or_else(|error| error.or(decode_pattern(node).map(Element::Pattern)))
}

pub fn decode_pattern(node: &Node) -> Result<Pattern, DecodeError> {
    decode_reference(node)
        .map(Pattern::Reference)
        .or_else(|error| error.or(decode_one_of(node).map(Pattern::OneOf)))
        .or_else(|error| error.or(decode_repetition(node).map(Pattern::Repetition)))
        .or_else(|error| error.or(decode_container(node).map(Pattern::Container)))
        .or_else(|error| error.or(decode_specific_call(node).map(Pattern::SpecificCall)))
        .or_else(|error| error.or(decode_any_call(node).map(Pattern::AnyCall)))
        .or_else(|error| error.or(decode_word(node).map(Pattern::Word)))
        .or_else(|error| error.or(decode_any_word(node).map(Pattern::AnyWord)))
        .or_else(|error| error.or(decode_string(node).map(Pattern::String)))
        .or_else(|error| error.or(decode_number(node).map(Pattern::Number)))
}

pub fn decode_reference(node: &Node) -> Result<Reference, DecodeError> {
    let (name, arguments) = support::any_call(
        node,
        |name| REGEX_2.is_match(name),
        "a call whose name matches the regex `^[a-z][a-z0-9]*( [a-z][a-z0-9]*)*$`",
    )?;
    Ok(Reference {
        name,
        arguments: arguments.to_vec(),
        span: node.span,
    })
}

pub fn decode_one_of(node: &Node) -> Result<OneOf, DecodeError> {
    let mut parts = support::call(node, "the call `ONE OF _`")?;
    let decoded = OneOf {
        choices: parts
            .word("ONE")?
            .word("OF")?
            .one(decode_one_of_choices, "`[...]`")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_one_of_choices(node: &Node) -> Result<Spanned<Vec<Element>>, DecodeError> {
    let mut elements = support::square_bracket(node)?;
    let value = elements.one_or_more(decode_element, "an element")?;
    elements.finish()?;
    Ok(Spanned {
        value,
        span: node.span,
    })
}

pub fn decode_repetition(node: &Node) -> Result<Repetition, DecodeError> {
    decode_repetition_zero_or_more(node)
        .or_else(|error| error.or(decode_repetition_one_or_more(node)))
        .or_else(|error| error.or(decode_repetition_optional(node)))
}

fn decode_repetition_zero_or_more(node: &Node) -> Result<Repetition, DecodeError> {
    let mut parts = support::call(node, "the call `ZERO OR MORE _`")?;
    let decoded = Repetition::ZeroOrMore {
        pattern: parts
            .word("ZERO")?
            .word("OR")?
            .word("MORE")?
            .one(|node| decode_pattern(node).map(Box::new), "a pattern")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_repetition_one_or_more(node: &Node) -> Result<Repetition, DecodeError> {
    let mut parts = support::call(node, "the call `ONE OR MORE _`")?;
    let decoded = Repetition::OneOrMore {
        pattern: parts
            .word("ONE")?
            .word("OR")?
            .word("MORE")?
            .one(|node| decode_pattern(node).map(Box::new), "a pattern")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_repetition_optional(node: &Node) -> Result<Repetition, DecodeError> {
    let mut parts = support::call(node, "the call `OPTIONAL _`")?;
    let decoded = Repetition::Optional {
        pattern: parts
            .word("OPTIONAL")?
            .one(|node| decode_pattern(node).map(Box::new), "a pattern")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

pub fn decode_container(node: &Node) -> Result<Container, DecodeError> {
    decode_container_curly_brace(node)
        .or_else(|error| error.or(decode_container_square_bracket(node)))
        .or_else(|error| error.or(decode_container_parenthesis(node)))
}

fn decode_container_curly_brace(node: &Node) -> Result<Container, DecodeError> {
    let mut parts = support::call(node, "the call `CURLY BRACE _`")?;
    let decoded = Container::CurlyBrace {
        contents: parts
            .word("CURLY")?
            .word("BRACE")?
            .one(|node| decode_contents(node).map(Box::new), "contents")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_container_square_bracket(node: &Node) -> Result<Container, DecodeError> {
    let mut parts = support::call(node, "the call `SQUARE BRACKET _`")?;
    let decoded = Container::SquareBracket {
        contents: parts
            .word("SQUARE")?
            .word("BRACKET")?
            .one(|node| decode_contents(node).map(Box::new), "contents")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_container_parenthesis(node: &Node) -> Result<Container, DecodeError> {
    let mut parts = support::call(node, "the call `PARENTHESIS _`")?;
    let decoded = Container::Parenthesis {
        contents: parts
            .word("PARENTHESIS")?
            .one(|node| decode_contents(node).map(Box::new), "contents")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

pub fn decode_specific_call(node: &Node) -> Result<SpecificCall, DecodeError> {
    let mut parts = support::call(node, "the call `SPECIFIC ANYFIX CALL _`")?;
    let decoded = SpecificCall {
        parts: parts
            .word("SPECIFIC")?
            .word("ANYFIX")?
            .word("CALL")?
            .one(decode_specific_call_parts, "`[...]`")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_specific_call_parts(node: &Node) -> Result<Spanned<Vec<Element>>, DecodeError> {
    let mut elements = support::square_bracket(node)?;
    let value = elements.one_or_more(decode_element, "an element")?;
    elements.finish()?;
    Ok(Spanned {
        value,
        span: node.span,
    })
}

pub fn decode_any_call(node: &Node) -> Result<AnyCall, DecodeError> {
    let mut parts = support::call(node, "the call `ANYFIX CALL _?`")?;
    let decoded = AnyCall {
        conditions: parts
            .word("ANYFIX")?
            .word("CALL")?
            .optional(decode_any_call_conditions),
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_any_call_conditions(node: &Node) -> Result<Spanned<Vec<CallCondition>>, DecodeError> {
    let mut elements = support::square_bracket(node)?;
    let value = elements.one_or_more(decode_call_condition, "a call condition")?;
    elements.finish()?;
    Ok(Spanned {
        value,
        span: node.span,
    })
}

pub fn decode_call_condition(node: &Node) -> Result<CallCondition, DecodeError> {
    decode_call_condition_name(node)
        .or_else(|error| error.or(decode_call_condition_arguments(node)))
}

fn decode_call_condition_name(node: &Node) -> Result<CallCondition, DecodeError> {
    let mut parts = support::call(node, "the call `NAME _`")?;
    let decoded = CallCondition::Name {
        conditions: parts
            .word("NAME")?
            .one(decode_call_condition_name_conditions, "`[...]`")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_call_condition_name_conditions(
    node: &Node,
) -> Result<Spanned<Vec<StringCondition>>, DecodeError> {
    let mut elements = support::square_bracket(node)?;
    let value = elements.one_or_more(decode_string_condition, "a string condition")?;
    elements.finish()?;
    Ok(Spanned {
        value,
        span: node.span,
    })
}

fn decode_call_condition_arguments(node: &Node) -> Result<CallCondition, DecodeError> {
    let mut parts = support::call(node, "the call `ARGUMENTS _`")?;
    let decoded = CallCondition::Arguments {
        pattern: parts.word("ARGUMENTS")?.one(decode_pattern, "a pattern")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

pub fn decode_word(node: &Node) -> Result<Word, DecodeError> {
    let mut parts = support::call(node, "the call `WORD _`")?;
    let decoded = Word {
        text: parts.word("WORD")?.one(
            |node| support::string(node, |_| true, "a string"),
            "a string",
        )?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

pub fn decode_any_word(node: &Node) -> Result<AnyWord, DecodeError> {
    let mut parts = support::call(node, "the call `ANY WORD _?`")?;
    let decoded = AnyWord {
        conditions: parts
            .word("ANY")?
            .word("WORD")?
            .optional(decode_any_word_conditions),
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_any_word_conditions(node: &Node) -> Result<Spanned<Vec<WordCondition>>, DecodeError> {
    let mut elements = support::square_bracket(node)?;
    let value = elements.one_or_more(decode_word_condition, "a word condition")?;
    elements.finish()?;
    Ok(Spanned {
        value,
        span: node.span,
    })
}

pub fn decode_word_condition(node: &Node) -> Result<WordCondition, DecodeError> {
    support::word(node, "KEYWORD")
        .map(WordCondition::Keyword)
        .or_else(|error| error.or(support::word(node, "NAME").map(WordCondition::Name)))
        .or_else(|error| error.or(support::word(node, "SYMBOL").map(WordCondition::Symbol)))
        .or_else(|error| {
            error.or(decode_string_condition(node).map(WordCondition::StringCondition))
        })
}

pub fn decode_string(node: &Node) -> Result<String, DecodeError> {
    support::word(node, "STRING")
        .map(String::Any)
        .or_else(|error| error.or(decode_string_exact(node)))
        .or_else(|error| error.or(decode_string_conditions(node)))
}

fn decode_string_exact(node: &Node) -> Result<String, DecodeError> {
    let mut parts = support::call(node, "the call `STRING _`")?;
    let decoded = String::Exact {
        text: parts.word("STRING")?.one(
            |node| support::string(node, |_| true, "a string"),
            "a string",
        )?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_string_conditions(node: &Node) -> Result<String, DecodeError> {
    let mut parts = support::call(node, "the call `STRING _`")?;
    let decoded = String::Conditions {
        conditions: parts
            .word("STRING")?
            .one(decode_string_conditions_conditions, "`[...]`")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_string_conditions_conditions(
    node: &Node,
) -> Result<Spanned<Vec<StringCondition>>, DecodeError> {
    let mut elements = support::square_bracket(node)?;
    let value = elements.one_or_more(decode_string_condition, "a string condition")?;
    elements.finish()?;
    Ok(Spanned {
        value,
        span: node.span,
    })
}

pub fn decode_string_condition(node: &Node) -> Result<StringCondition, DecodeError> {
    decode_string_condition_starts_with(node)
        .or_else(|error| error.or(decode_string_condition_ends_with(node)))
        .or_else(|error| error.or(decode_string_condition_contains(node)))
        .or_else(|error| error.or(decode_string_condition_matching_regex(node)))
}

fn decode_string_condition_starts_with(node: &Node) -> Result<StringCondition, DecodeError> {
    let mut parts = support::call(node, "the call `STARTS WITH _`")?;
    let decoded = StringCondition::StartsWith {
        text: parts.word("STARTS")?.word("WITH")?.one(
            |node| support::string(node, |_| true, "a string"),
            "a string",
        )?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_string_condition_ends_with(node: &Node) -> Result<StringCondition, DecodeError> {
    let mut parts = support::call(node, "the call `ENDS WITH _`")?;
    let decoded = StringCondition::EndsWith {
        text: parts.word("ENDS")?.word("WITH")?.one(
            |node| support::string(node, |_| true, "a string"),
            "a string",
        )?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_string_condition_contains(node: &Node) -> Result<StringCondition, DecodeError> {
    let mut parts = support::call(node, "the call `CONTAINS _`")?;
    let decoded = StringCondition::Contains {
        text: parts.word("CONTAINS")?.one(
            |node| support::string(node, |_| true, "a string"),
            "a string",
        )?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_string_condition_matching_regex(node: &Node) -> Result<StringCondition, DecodeError> {
    let mut parts = support::call(node, "the call `MATCHING REGEX _`")?;
    let decoded = StringCondition::MatchingRegex {
        regex: parts.word("MATCHING")?.word("REGEX")?.one(
            |node| support::string(node, |_| true, "a string"),
            "a string",
        )?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

pub fn decode_number(node: &Node) -> Result<Number, DecodeError> {
    support::word(node, "NUMBER")
        .map(Number::Any)
        .or_else(|error| error.or(decode_number_exact(node)))
        .or_else(|error| error.or(decode_number_conditions(node)))
}

fn decode_number_exact(node: &Node) -> Result<Number, DecodeError> {
    let mut parts = support::call(node, "the call `NUMBER _`")?;
    let decoded = Number::Exact {
        value: parts.word("NUMBER")?.one(
            |node| support::float(node, f64::NEG_INFINITY, f64::INFINITY, "a number"),
            "a number",
        )?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_number_conditions(node: &Node) -> Result<Number, DecodeError> {
    let mut parts = support::call(node, "the call `NUMBER _`")?;
    let decoded = Number::Conditions {
        conditions: parts
            .word("NUMBER")?
            .one(decode_number_conditions_conditions, "`[...]`")?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_number_conditions_conditions(
    node: &Node,
) -> Result<Spanned<Vec<NumberCondition>>, DecodeError> {
    let mut elements = support::square_bracket(node)?;
    let value = elements.one_or_more(decode_number_condition, "a number condition")?;
    elements.finish()?;
    Ok(Spanned {
        value,
        span: node.span,
    })
}

pub fn decode_number_condition(node: &Node) -> Result<NumberCondition, DecodeError> {
    support::word(node, "INTEGER")
        .map(NumberCondition::Integer)
        .or_else(|error| error.or(decode_number_condition_at_least(node)))
        .or_else(|error| error.or(decode_number_condition_at_most(node)))
}

fn decode_number_condition_at_least(node: &Node) -> Result<NumberCondition, DecodeError> {
    let mut parts = support::call(node, "the call `AT LEAST _`")?;
    let decoded = NumberCondition::AtLeast {
        bound: parts.word("AT")?.word("LEAST")?.one(
            |node| support::float(node, f64::NEG_INFINITY, f64::INFINITY, "a number"),
            "a number",
        )?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}

fn decode_number_condition_at_most(node: &Node) -> Result<NumberCondition, DecodeError> {
    let mut parts = support::call(node, "the call `AT MOST _`")?;
    let decoded = NumberCondition::AtMost {
        bound: parts.word("AT")?.word("MOST")?.one(
            |node| support::float(node, f64::NEG_INFINITY, f64::INFINITY, "a number"),
            "a number",
        )?,
        span: node.span,
    };
    parts.finish()?;
    Ok(decoded)
}
