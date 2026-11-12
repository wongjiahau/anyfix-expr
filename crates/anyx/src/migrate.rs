//! Rewrites sources written before `#` started a line string.

use crate::lexer::lex;
use crate::syntax::TokenKind;

/// `source`, written when `#` started a comment, with each of those comments
/// turned into a `##` comment. Before line strings, every `#` outside a string
/// started a comment. Such a `#` now starts a line string (or, as `##`, is
/// already a comment), so adding a `#` to each line of every line string
/// restores the comments. Text inside other strings is unchanged.
///
/// ```
/// let old = "port: 80, # the default\n# for the server\nhost: \"#1\",\n";
/// assert_eq!(
///     anyx::migrate_comments(old),
///     "port: 80, ## the default\n## for the server\nhost: \"#1\",\n",
/// );
/// ```
pub fn migrate_comments(source: &str) -> String {
    let (tokens, _) = lex(source);
    let hashes: Vec<usize> = tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::String(_)) && token.text.starts_with('#'))
        .flat_map(|token| {
            token
                .text
                .split('\n')
                .scan(token.span.start, |line_start, line| {
                    let hash = *line_start + line.len() - line.trim_start().len();
                    *line_start += line.len() + 1;
                    Some(hash)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let mut migrated = String::with_capacity(source.len() + hashes.len());
    let mut copied = 0;
    for hash in hashes {
        migrated.push_str(&source[copied..hash]);
        migrated.push('#');
        copied = hash;
    }
    migrated.push_str(&source[copied..]);
    migrated
}
