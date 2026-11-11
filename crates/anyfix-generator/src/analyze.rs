//! Checks on a whole grammar: every rule is used, no rule calls itself
//! forever, and no two choices of a `ONE OF` compete for the same input.

use crate::diagnostic::Diagnostic;
use crate::model::{Element, Grammar, Pattern};
use crate::overlap::Overlap;

pub fn analyze(grammar: &Grammar) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    unreachable_rules(grammar, &mut diagnostics);
    endless_recursion(grammar, &mut diagnostics);
    let mut overlap = Overlap::new(grammar);
    let roots = grammar
        .entry
        .iter()
        .map(|element| &element.pattern)
        .chain(grammar.rules.iter().map(|rule| &rule.pattern));
    for root in roots {
        competing_choices(root, &mut overlap, &mut diagnostics);
    }
    diagnostics
}

/// The patterns directly inside `pattern`.
fn children(pattern: &Pattern) -> Vec<&Pattern> {
    fn of(elements: &[Element]) -> Vec<&Pattern> {
        elements.iter().map(|element| &element.pattern).collect()
    }
    match pattern {
        Pattern::OneOf(choices) => choices.iter().map(|choice| &choice.pattern).collect(),
        Pattern::Container { elements, .. } => of(elements),
        Pattern::SpecificCall(parts) => of(parts),
        Pattern::AnyCall { arguments, .. } => arguments
            .iter()
            .map(|arguments| &arguments.pattern)
            .collect(),
        Pattern::Reference(_)
        | Pattern::Word(_)
        | Pattern::AnyWord { .. }
        | Pattern::String(_)
        | Pattern::Number(_) => Vec::new(),
    }
}

fn unreachable_rules(grammar: &Grammar, diagnostics: &mut Vec<Diagnostic>) {
    let mut reached = vec![false; grammar.rules.len()];
    let mut pending: Vec<&Pattern> = grammar
        .entry
        .iter()
        .map(|element| &element.pattern)
        .collect();
    while let Some(pattern) = pending.pop() {
        if let Pattern::Reference(rule) = pattern
            && !std::mem::replace(&mut reached[*rule], true)
        {
            pending.push(&grammar.rules[*rule].pattern);
        }
        pending.extend(children(pattern));
    }
    for (rule, reached) in grammar.rules.iter().zip(reached) {
        if !reached {
            diagnostics.push(Diagnostic::warning(
                rule.name_span,
                format!(
                    "found the rule `{}`, which nothing reachable from `ENTRY` refers to; expected every rule to be used; refer to it or remove it",
                    rule.name
                ),
            ));
        }
    }
}

/// The rules `pattern` hands the very same node to: a reference, or a
/// reference among the choices of a `ONE OF`.
fn same_node_rules(pattern: &Pattern, rules: &mut Vec<usize>) {
    match pattern {
        Pattern::Reference(rule) => rules.push(*rule),
        Pattern::OneOf(choices) => {
            for choice in choices {
                same_node_rules(&choice.pattern, rules);
            }
        }
        _ => {}
    }
}

/// A rule that reaches itself without going into a call or a bracket would
/// decode the same node forever.
fn endless_recursion(grammar: &Grammar, diagnostics: &mut Vec<Diagnostic>) {
    let edges: Vec<Vec<usize>> = grammar
        .rules
        .iter()
        .map(|rule| {
            let mut rules = Vec::new();
            same_node_rules(&rule.pattern, &mut rules);
            rules
        })
        .collect();
    let mut reported = vec![false; grammar.rules.len()];
    for start in 0..grammar.rules.len() {
        if reported[start] {
            continue;
        }
        let Some(cycle) = cycle_through(start, &edges) else {
            continue;
        };
        for rule in &cycle {
            reported[*rule] = true;
        }
        let path: Vec<&str> = cycle
            .iter()
            .chain([&start])
            .map(|rule| grammar.rules[*rule].name.as_str())
            .collect();
        diagnostics.push(Diagnostic::error(
            grammar.rules[start].name_span,
            format!(
                "found the rule `{}` referring to itself with nothing matched in between ({}); expected a call or a bracket on the way, so that each step looks at a smaller node; put the reference inside one, or remove it",
                grammar.rules[start].name,
                path.join(" → ")
            ),
        ));
    }
}

/// A path of rules from `start` back to `start`, if there is one.
fn cycle_through(start: usize, edges: &[Vec<usize>]) -> Option<Vec<usize>> {
    fn visit(
        rule: usize,
        start: usize,
        edges: &[Vec<usize>],
        visited: &mut Vec<bool>,
        path: &mut Vec<usize>,
    ) -> bool {
        path.push(rule);
        for next in &edges[rule] {
            if *next == start {
                return true;
            }
            if !std::mem::replace(&mut visited[*next], true)
                && visit(*next, start, edges, visited, path)
            {
                return true;
            }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    let mut visited = vec![false; edges.len()];
    visit(start, start, edges, &mut visited, &mut path).then_some(path)
}

fn competing_choices(pattern: &Pattern, overlap: &mut Overlap, diagnostics: &mut Vec<Diagnostic>) {
    if let Pattern::OneOf(choices) = pattern {
        for (index, later) in choices.iter().enumerate() {
            let earlier = choices[..index]
                .iter()
                .find(|earlier| overlap.patterns(&earlier.pattern, &later.pattern));
            if let Some(earlier) = earlier {
                diagnostics.push(Diagnostic::warning(
                    later.span,
                    format!(
                        "found the choices `{}` and `{}` able to match the same input; expected at most one choice to fit any input, since `{}` comes first and always wins it; make one of them more specific, or put the more specific one first",
                        earlier.name, later.name, earlier.name
                    ),
                ));
            }
        }
    }
    for child in children(pattern) {
        competing_choices(child, overlap, diagnostics);
    }
}
