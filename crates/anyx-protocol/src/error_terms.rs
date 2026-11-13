//! Reads an error list into its terms: own errors in brackets, after a
//! group's name in an `ERRORS` group, then the groups it joins with `+`.
//! Joined terms make one call, `_ + _`, in which a full path such as
//! `files/file errors` reads as `_/_`, so the terms come from that call's
//! name.

use anyx::{NamePart, Span};

use crate::grammar as ast;

/// An error list, read.
pub(crate) struct ErrorTerms<'a> {
    /// The group's name, written before its own errors:
    /// `file errors [trashed, missing]`.
    pub(crate) name: Option<ast::Spanned<String>>,
    /// The errors defined in the brackets.
    pub(crate) own: &'a [ast::Error],
    /// The groups joined with `+`, in order.
    pub(crate) groups: Vec<GroupReference>,
}

/// A group joined with `+`, by its name or its full path.
#[derive(Clone)]
pub(crate) struct GroupReference {
    /// The scope before its name, empty when written without one.
    pub(crate) scope: Vec<String>,
    pub(crate) name: String,
    pub(crate) span: Span,
}

/// What a list holds that it can't.
pub(crate) enum Misplaced {
    /// Something that is neither errors in brackets nor a group.
    NotATerm(Span),
    /// Errors in brackets after a `+`.
    OwnAfterGroup(Span),
}

pub(crate) fn read(list: &ast::ErrorTerm) -> Result<ErrorTerms<'_>, Misplaced> {
    let mut terms = ErrorTerms {
        name: None,
        own: &[],
        groups: Vec::new(),
    };
    for (index, joined) in joined_terms(list)?.into_iter().enumerate() {
        match (joined.as_slice(), index) {
            ([term], first) => match own_errors(term) {
                Some((name, own)) if first == 0 => {
                    terms.name = name;
                    terms.own = own;
                }
                Some(_) => return Err(Misplaced::OwnAfterGroup(span(term))),
                None => terms.groups.push(group(&joined)?),
            },
            (path, _) => terms.groups.push(group(path)?),
        }
    }
    Ok(terms)
}

/// The terms joined by `+`, each as the parts of its path, which `/` joins.
fn joined_terms(list: &ast::ErrorTerm) -> Result<Vec<Vec<&ast::ErrorTerm>>, Misplaced> {
    let ast::ErrorTerm::Call {
        name, arguments, ..
    } = list
    else {
        return Ok(vec![vec![list]]);
    };
    let symbols: Vec<&str> = name
        .parts
        .iter()
        .filter_map(|part| match part {
            NamePart::Word { text, .. } if !text.starts_with(char::is_alphanumeric) => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect();
    if symbols.is_empty() {
        return Ok(vec![vec![list]]);
    }
    let slots_between_symbols = name.parts.len() + 1 == 2 * arguments.len()
        && name
            .parts
            .iter()
            .enumerate()
            .all(|(index, part)| match part {
                NamePart::Slot => index % 2 == 0,
                NamePart::Word { text, .. } => index % 2 == 1 && (text == "+" || text == "/"),
            });
    if !slots_between_symbols {
        return Err(Misplaced::NotATerm(span(list)));
    }
    let mut joined = vec![vec![&arguments[0]]];
    for (symbol, argument) in symbols.iter().zip(&arguments[1..]) {
        match *symbol {
            "+" => joined.push(vec![argument]),
            _ => joined
                .last_mut()
                .expect("starts with one term")
                .push(argument),
        }
    }
    Ok(joined)
}

/// The errors in brackets, after the group's name if it has one, when
/// `term` is that.
fn own_errors(term: &ast::ErrorTerm) -> Option<(Option<ast::Spanned<String>>, &[ast::Error])> {
    match term {
        ast::ErrorTerm::Own(errors) => Some((None, &errors.value)),
        ast::ErrorTerm::Call {
            name, arguments, ..
        } => match (name.parts.split_last(), arguments.as_slice()) {
            (Some((NamePart::Slot, words)), [ast::ErrorTerm::Own(errors)])
                if !words.is_empty() && !words.contains(&NamePart::Slot) =>
            {
                let name = ast::Spanned {
                    value: name.words().collect::<Vec<_>>().join(" "),
                    span: name.span,
                };
                Some((Some(name), &errors.value))
            }
            _ => None,
        },
    }
}

/// A group by its name, or by its scope's path then its name.
fn group(path: &[&ast::ErrorTerm]) -> Result<GroupReference, Misplaced> {
    let words = path
        .iter()
        .map(|term| match term {
            ast::ErrorTerm::Call {
                name, arguments, ..
            } if arguments.is_empty() && !name.parts.contains(&NamePart::Slot) => {
                Ok(name.words().collect::<Vec<_>>().join(" "))
            }
            _ => Err(Misplaced::NotATerm(span(term))),
        })
        .collect::<Result<Vec<String>, Misplaced>>()?;
    let (name, scope) = words.split_last().expect("a path has a part");
    Ok(GroupReference {
        scope: scope.to_vec(),
        name: name.clone(),
        span: Span {
            start: span(path[0]).start,
            end: span(path[path.len() - 1]).end,
        },
    })
}

pub(crate) fn span(term: &ast::ErrorTerm) -> Span {
    match term {
        ast::ErrorTerm::Own(errors) => errors.span,
        ast::ErrorTerm::Call { span, .. } => *span,
    }
}
