//! Whether two patterns can match the same node, which is what makes the
//! order of a `ONE OF` matter. The answer leans towards yes: when it can't
//! tell, it says they can.

use std::collections::HashSet;

use regex_automata::Anchored;
use regex_automata::dfa::{Automaton, dense};
use regex_automata::util::primitives::StateID;
use regex_automata::util::start;

use crate::model::{
    Element, Grammar, NumberPattern, NumberType, Pattern, Repetition, StringCondition, WordKind,
};

pub struct Overlap<'a> {
    grammar: &'a Grammar,
    /// The pairs being compared, to stop at recursive rules. A node is a
    /// finite tree, so a pair that only overlaps through itself does not.
    active: Vec<(usize, usize)>,
    /// The same for [`Overlap::covers`].
    covering: Vec<(usize, usize)>,
}

/// One step of a sequence: a name word of a call, or a node.
#[derive(Clone, Copy)]
enum Atom<'a> {
    NameWord(&'a str),
    Node(&'a Pattern),
    /// Any node: an element of `EACH`, which keeps a node that does not fit
    /// as its error.
    AnyNode,
}

#[derive(Clone, Copy)]
struct Step<'a> {
    atom: Atom<'a>,
    optional: bool,
    repeats: bool,
}

impl<'a> Overlap<'a> {
    pub fn new(grammar: &'a Grammar) -> Self {
        Overlap {
            grammar,
            active: Vec::new(),
            covering: Vec::new(),
        }
    }

    /// Whether `general` matches every node `specific` matches, which makes
    /// `general` after `specific` a fallback rather than a competitor. Of
    /// the choices `specific` has, only those `general` can match count. The
    /// answer leans towards no: when it can't tell, it says it doesn't.
    pub fn covers(&mut self, general: &Pattern, specific: &Pattern) -> bool {
        let key = (
            std::ptr::from_ref(general) as usize,
            std::ptr::from_ref(specific) as usize,
        );
        if self.covering.contains(&key) {
            return false;
        }
        self.covering.push(key);
        let covers = self.compare_cover(general, specific);
        self.covering.pop();
        covers
    }

    fn compare_cover(&mut self, general: &Pattern, specific: &Pattern) -> bool {
        let grammar = self.grammar;
        if general == specific || *general == Pattern::AnyNode {
            return true;
        }
        match (general, specific) {
            (_, Pattern::Reference(rule)) => self.covers(general, &grammar.rules[*rule].pattern),
            // A choice `general` can't match at all needs no covering.
            (_, Pattern::OneOf(choices)) => choices.iter().all(|choice| {
                !self.patterns(general, &choice.pattern) || self.covers(general, &choice.pattern)
            }),
            (Pattern::Reference(rule), _) => self.covers(&grammar.rules[*rule].pattern, specific),
            (Pattern::OneOf(choices), _) => choices
                .iter()
                .any(|choice| self.covers(&choice.pattern, specific)),
            (
                Pattern::AnyCall {
                    name,
                    arguments: None,
                },
                _,
            ) => call_name_covers(name, specific),
            _ => false,
        }
    }

    pub fn patterns(&mut self, a: &Pattern, b: &Pattern) -> bool {
        let key = (
            std::ptr::from_ref(a) as usize,
            std::ptr::from_ref(b) as usize,
        );
        if self.active.contains(&key) {
            return false;
        }
        self.active.push(key);
        let overlap = self.compare(a, b);
        self.active.pop();
        overlap
    }

    fn compare(&mut self, a: &Pattern, b: &Pattern) -> bool {
        let grammar = self.grammar;
        match (a, b) {
            (Pattern::Reference(rule), other) => {
                self.patterns(&grammar.rules[*rule].pattern, other)
            }
            (other, Pattern::Reference(rule)) => {
                self.patterns(other, &grammar.rules[*rule].pattern)
            }
            (Pattern::OneOf(choices), other) => choices
                .iter()
                .any(|choice| self.patterns(&choice.pattern, other)),
            (other, Pattern::OneOf(choices)) => choices
                .iter()
                .any(|choice| self.patterns(other, &choice.pattern)),
            (Pattern::AnyNode, _) | (_, Pattern::AnyNode) => true,
            (
                Pattern::Container { kind, elements },
                Pattern::Container {
                    kind: other_kind,
                    elements: other_elements,
                },
            ) => {
                kind == other_kind
                    && self.sequences(&node_steps(elements), &node_steps(other_elements))
            }
            (Pattern::String(a), Pattern::String(b)) => conditions_overlap(a, b),
            (Pattern::Number(a), Pattern::Number(b)) => numbers_overlap(a, b),
            (Pattern::Container { .. } | Pattern::String(_) | Pattern::Number(_), _)
            | (_, Pattern::Container { .. } | Pattern::String(_) | Pattern::Number(_)) => false,
            _ => self.calls(a, b) || self.calls(b, a),
        }
    }

    /// Two patterns that match calls. Each pair is handled in one order
    /// only, so the caller tries both.
    fn calls(&mut self, a: &Pattern, b: &Pattern) -> bool {
        match (a, b) {
            (Pattern::Word(a), Pattern::Word(b)) => a == b,
            (Pattern::Word(text), Pattern::AnyWord { kind, conditions }) => {
                kind.is_none_or(|kind| kind == word_kind(text))
                    && conditions.iter().all(|condition| condition.holds(text))
            }
            (Pattern::Word(text), Pattern::AnyCall { name, arguments }) => {
                name.iter().all(|condition| condition.holds(text))
                    && arguments
                        .as_ref()
                        .is_none_or(|arguments| arguments.repetition.may_be_empty())
            }
            (Pattern::Word(_) | Pattern::AnyWord { .. }, Pattern::SpecificCall(parts)) => {
                match only_word(parts) {
                    Some(word) => {
                        let word = Pattern::Word(word.to_string());
                        self.calls(&word, a) || self.calls(a, &word)
                    }
                    None => false,
                }
            }
            (
                Pattern::AnyWord { kind, conditions },
                Pattern::AnyWord {
                    kind: other_kind,
                    conditions: other_conditions,
                },
            ) => {
                (kind.is_none() || other_kind.is_none() || kind == other_kind)
                    && conditions_overlap(conditions, other_conditions)
            }
            (Pattern::AnyWord { conditions, .. }, Pattern::AnyCall { name, arguments }) => {
                conditions_overlap(conditions, name)
                    && arguments
                        .as_ref()
                        .is_none_or(|arguments| arguments.repetition.may_be_empty())
            }
            (
                Pattern::AnyCall { name, arguments },
                Pattern::AnyCall {
                    name: other_name,
                    arguments: other_arguments,
                },
            ) => {
                conditions_overlap(name, other_name)
                    && match (arguments, other_arguments) {
                        (Some(a), Some(b)) => self.sequences(
                            &node_steps(std::slice::from_ref(a)),
                            &node_steps(std::slice::from_ref(b)),
                        ),
                        _ => true,
                    }
            }
            (Pattern::AnyCall { name, arguments }, Pattern::SpecificCall(parts)) => {
                let fits_a_name = possible_names(parts)
                    .iter()
                    .any(|text| name.iter().all(|condition| condition.holds(text)));
                let slots: Vec<Element> = parts
                    .iter()
                    .filter(|part| !matches!(part.pattern, Pattern::Word(_)))
                    .cloned()
                    .collect();
                fits_a_name
                    && arguments.as_ref().is_none_or(|arguments| {
                        self.sequences(
                            &node_steps(&slots),
                            &node_steps(std::slice::from_ref(arguments)),
                        )
                    })
            }
            (Pattern::SpecificCall(a), Pattern::SpecificCall(b)) => {
                self.sequences(&part_steps(a), &part_steps(b))
            }
            _ => false,
        }
    }

    /// Whether some list of items fits both sequences.
    fn sequences(&mut self, a: &[Step], b: &[Step]) -> bool {
        let mut seen = HashSet::new();
        let mut pending = vec![(0, 0)];
        while let Some((i, j)) = pending.pop() {
            if !seen.insert((i, j)) {
                continue;
            }
            match (a.get(i), b.get(j)) {
                (None, None) => return true,
                (Some(step), _) if step.optional => pending.push((i + 1, j)),
                _ => {}
            }
            if b.get(j).is_some_and(|step| step.optional) {
                pending.push((i, j + 1));
            }
            let (Some(x), Some(y)) = (a.get(i), b.get(j)) else {
                continue;
            };
            let both = match (x.atom, y.atom) {
                (Atom::NameWord(x), Atom::NameWord(y)) => x == y,
                (Atom::Node(x), Atom::Node(y)) => self.patterns(x, y),
                (Atom::AnyNode, Atom::Node(_) | Atom::AnyNode) | (Atom::Node(_), Atom::AnyNode) => {
                    true
                }
                _ => false,
            };
            if both {
                for next_i in [i + 1, i] {
                    for next_j in [j + 1, j] {
                        if (next_i == i + 1 || x.repeats) && (next_j == j + 1 || y.repeats) {
                            pending.push((next_i, next_j));
                        }
                    }
                }
            }
        }
        false
    }
}

/// The steps of a bracket's elements. `ONE OR MORE: x` is `x`, then
/// `ZERO OR MORE: x`.
fn node_steps(elements: &[Element]) -> Vec<Step<'_>> {
    steps(elements, |element| Atom::Node(&element.pattern))
}

/// The steps of a call's parts, where a `WORD` is a name word.
fn part_steps(parts: &[Element]) -> Vec<Step<'_>> {
    steps(parts, |part| match &part.pattern {
        Pattern::Word(text) => Atom::NameWord(text),
        pattern => Atom::Node(pattern),
    })
}

fn steps<'a>(elements: &'a [Element], atom: impl Fn(&'a Element) -> Atom<'a>) -> Vec<Step<'a>> {
    let mut steps = Vec::new();
    for element in elements {
        let atom = match element.repetition {
            Repetition::Each => Atom::AnyNode,
            _ => atom(element),
        };
        let step = |optional, repeats| Step {
            atom,
            optional,
            repeats,
        };
        match element.repetition {
            Repetition::One => steps.push(step(false, false)),
            Repetition::Optional => steps.push(step(true, false)),
            Repetition::ZeroOrMore | Repetition::Each => steps.push(step(true, true)),
            Repetition::OneOrMore => steps.extend([step(false, false), step(true, true)]),
        }
    }
    steps
}

/// Whether a call whose name meets every condition in `conditions`, with
/// any arguments, takes every node `specific` matches.
fn call_name_covers(conditions: &[StringCondition], specific: &Pattern) -> bool {
    let all_hold = |text: &str| conditions.iter().all(|condition| condition.holds(text));
    match specific {
        Pattern::Word(text) => all_hold(text),
        Pattern::AnyWord {
            conditions: specific_conditions,
            ..
        }
        | Pattern::AnyCall {
            name: specific_conditions,
            ..
        } => conditions
            .iter()
            .all(|condition| specific_conditions.contains(condition)),
        // The names tried stop at two of a repeated slot, so only a call
        // without one is checked name by name.
        Pattern::SpecificCall(parts) => {
            conditions.is_empty()
                || (!parts.iter().any(|part| part.repetition.may_repeat())
                    && possible_names(parts).iter().all(|name| all_hold(name)))
        }
        _ => false,
    }
}

/// The word a specific call is when it can be one word and nothing else.
fn only_word(parts: &[Element]) -> Option<&str> {
    let mut words = parts.iter().filter_map(|part| match &part.pattern {
        Pattern::Word(text) => Some(text.as_str()),
        _ => None,
    });
    let word = words.next()?;
    let alone = words.next().is_none()
        && parts
            .iter()
            .all(|part| matches!(part.pattern, Pattern::Word(_)) || part.repetition.may_be_empty());
    alone.then_some(word)
}

/// Name texts a specific call can have, such as `print`, `print _` and
/// `print _ _`. A slot that repeats is tried up to twice.
fn possible_names(parts: &[Element]) -> Vec<String> {
    let mut names = vec![Vec::new()];
    for part in parts {
        let counts: &[usize] = match (&part.pattern, part.repetition) {
            (Pattern::Word(_), _) | (_, Repetition::One) => &[1],
            (_, Repetition::Optional) => &[0, 1],
            (_, Repetition::ZeroOrMore | Repetition::Each) => &[0, 1, 2],
            (_, Repetition::OneOrMore) => &[1, 2],
        };
        let text = match &part.pattern {
            Pattern::Word(text) => text.as_str(),
            _ => "_",
        };
        names = names
            .iter()
            .flat_map(|name: &Vec<&str>| {
                counts.iter().map(move |count| {
                    let mut name = name.clone();
                    name.extend(std::iter::repeat_n(text, *count));
                    name
                })
            })
            .take(256)
            .collect();
    }
    names.iter().map(|name| name.join(" ")).collect()
}

fn word_kind(text: &str) -> WordKind {
    if anyfix_expr::is_symbol_word(text) {
        WordKind::Symbol
    } else if anyfix_expr::is_keyword(text) {
        WordKind::Keyword
    } else {
        WordKind::Name
    }
}

fn numbers_overlap(a: &NumberPattern, b: &NumberPattern) -> bool {
    let highest_minimum = match (a.minimum, b.minimum) {
        (Some(a), Some(b)) if a.as_float() < b.as_float() => Some(b),
        (Some(a), _) => Some(a),
        (None, b) => b,
    };
    let lowest_maximum = match (a.maximum, b.maximum) {
        (Some(a), Some(b)) if a.as_float() > b.as_float() => Some(b),
        (Some(a), _) => Some(a),
        (None, b) => b,
    };
    let both = NumberPattern {
        integer: a.integer || b.integer,
        minimum: highest_minimum,
        maximum: lowest_maximum,
    };
    match both.number_type() {
        NumberType::Integer {
            minimum, maximum, ..
        } => minimum <= maximum,
        NumberType::Float { minimum, maximum } => minimum <= maximum,
    }
}

/// Whether some text meets every condition of both lists.
fn conditions_overlap(a: &[StringCondition], b: &[StringCondition]) -> bool {
    let all: Vec<&StringCondition> = a.iter().chain(b).collect();
    let exact = all.iter().find_map(|condition| match condition {
        StringCondition::Is(text) => Some(text),
        _ => None,
    });
    if let Some(text) = exact {
        return all.iter().all(|condition| condition.holds(text));
    }
    let regexes: Vec<String> = all
        .iter()
        .map(|condition| match condition {
            StringCondition::Is(text) => format!("^(?:{})$", regex::escape(text)),
            StringCondition::StartsWith(text) => format!("^(?:{})", regex::escape(text)),
            StringCondition::EndsWith(text) => format!("(?:{})$", regex::escape(text)),
            StringCondition::Contains(text) => regex::escape(text),
            StringCondition::MatchingRegex(regex) => regex.clone(),
            StringCondition::NoSlots => "^(?:(?:[^ _][^ ]*|_[^ ]+)(?: |$))*$".to_string(),
        })
        .collect();
    some_text_matches_all(&regexes)
}

/// Whether some text matches every regex, found by walking their automata
/// side by side. Says yes when a regex is too large to walk.
fn some_text_matches_all(regexes: &[String]) -> bool {
    const MOST_STATES: usize = 20_000;
    let config = dense::Config::new()
        .dfa_size_limit(Some(1 << 22))
        .determinize_size_limit(Some(1 << 22));
    let mut automata = Vec::new();
    let mut starts = Vec::new();
    for regex in regexes {
        let Ok(automaton) = dense::Builder::new().configure(config.clone()).build(regex) else {
            return true;
        };
        let Ok(start) = automaton.start_state(&start::Config::new().anchored(Anchored::No)) else {
            return true;
        };
        automata.push(automaton);
        starts.push(start);
    }
    // A state per automaton, and whether each has matched yet. One that has
    // matched goes back to its start, to keep the states few.
    type Product = (Vec<StateID>, Vec<bool>);
    let first: Product = (starts.clone(), vec![false; automata.len()]);
    let mut seen: HashSet<Product> = HashSet::from([first.clone()]);
    let mut pending = vec![first];
    while let Some((states, matched)) = pending.pop() {
        let matches_at_end = automata.iter().enumerate().all(|(index, automaton)| {
            matched[index] || automaton.is_match_state(automaton.next_eoi_state(states[index]))
        });
        if matches_at_end {
            return true;
        }
        if seen.len() > MOST_STATES {
            return true;
        }
        'bytes: for byte in 0..=255u8 {
            let mut next_states = states.clone();
            let mut next_matched = matched.clone();
            for (index, automaton) in automata.iter().enumerate() {
                if matched[index] {
                    continue;
                }
                let next = automaton.next_state(states[index], byte);
                if automaton.is_quit_state(next) {
                    return true;
                }
                if automaton.is_match_state(next) {
                    next_matched[index] = true;
                    next_states[index] = starts[index];
                } else if automaton.is_dead_state(next) {
                    continue 'bytes;
                } else {
                    next_states[index] = next;
                }
            }
            let next = (next_states, next_matched);
            if seen.insert(next.clone()) {
                pending.push(next);
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::support::NumberValue;

    fn regexes(a: &str, b: &str) -> bool {
        some_text_matches_all(&[a.to_string(), b.to_string()])
    }

    #[test]
    fn regexes_overlap_when_one_text_matches_both() {
        assert!(regexes("^a+$", "^aa$"));
        assert!(regexes("a", "b"));
        assert!(regexes("^[a-z]+$", "^[a-c]x$"));
        assert!(!regexes("^a+$", "^b+$"));
        assert!(!regexes("^[a-z]+ _$", "^[a-z]+$"));
        assert!(!regexes("^a", "^b"));
        assert!(!regexes("a$", "b$"));
        assert!(!regexes("^\\d+$", "^[a-z]+$"));
        assert!(regexes("^\\d+$", "5"));
    }

    #[test]
    fn string_conditions_overlap_when_one_text_meets_all() {
        let starts = |text: &str| StringCondition::StartsWith(text.to_string());
        let is = |text: &str| StringCondition::Is(text.to_string());
        assert!(conditions_overlap(&[starts("ab")], &[starts("a")]));
        assert!(!conditions_overlap(&[starts("ab")], &[starts("b")]));
        assert!(conditions_overlap(&[is("abc")], &[starts("a")]));
        assert!(!conditions_overlap(&[is("abc")], &[is("abd")]));
        assert!(conditions_overlap(&[], &[is("abd")]));
    }

    #[test]
    fn numbers_overlap_when_their_ranges_share_a_value() {
        let number = |integer, minimum: Option<f64>, maximum: Option<f64>| NumberPattern {
            integer,
            minimum: minimum.map(NumberValue::Float),
            maximum: maximum.map(NumberValue::Float),
        };
        assert!(numbers_overlap(
            &number(false, None, None),
            &number(true, Some(0.0), Some(1.0))
        ));
        assert!(!numbers_overlap(
            &number(false, None, Some(1.0)),
            &number(false, Some(2.0), None)
        ));
        assert!(!numbers_overlap(
            &number(true, None, None),
            &number(false, Some(0.2), Some(0.8))
        ));
    }
}
