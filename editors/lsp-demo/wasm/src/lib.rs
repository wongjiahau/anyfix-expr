//! The poentaro language server for the demo page, from the same generated
//! decoder as `crates/anyx-lsp/examples/poentaro`.

#[path = "../../../../crates/anyx-lsp/examples/poentaro/generated.rs"]
#[rustfmt::skip]
#[allow(clippy::all, dead_code)]
mod generated;

anyx_lsp::language!(Poentaro = generated);
anyx_lsp::wasm!(Poentaro);
