//! A parser generator for anyfix expressions. It reads a grammar written in
//! anyfix and writes Rust: a typed AST, plus a decoder that turns the tree
//! from `anyfix_expr::SyntaxTree::nodes()` into that AST. See `README.md`
//! for the grammar language.
