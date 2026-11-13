//! A language server for `.poentaro` scores, from nothing but their grammar.
//!
//! `generated.rs` is what `anyx-generate poentaro.anyx` writes; the grammar
//! is copied from the poentaro repository.

#[rustfmt::skip]
#[allow(clippy::all)]
mod generated;

anyx_lsp::language!(Poentaro = generated);

fn main() {
    if let Err(error) = anyx_lsp::serve(&Poentaro) {
        eprintln!("poentaro-lsp: {error}");
        std::process::exit(1);
    }
}
