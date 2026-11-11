//! `anyfix-generate grammar.anyfix > src/parser.rs`

use std::process::ExitCode;

const USAGE: &str = "usage: anyfix-generate [--unformatted] <grammar.anyfix>

Reads a grammar written in anyfix and prints Rust: a type for every rule,
and a decoder from an anyfix syntax tree into those types.

    anyfix-generate grammar.anyfix > src/parser.rs

The output is laid out with rustfmt. With --unformatted it is printed as
generated, which needs no rustfmt.";

fn main() -> ExitCode {
    let mut unformatted = false;
    let mut path = None;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--unformatted" => unformatted = true,
            _ if argument.starts_with('-') || path.is_some() => {
                eprintln!("found the argument `{argument}`; expected one grammar file\n\n{USAGE}");
                return ExitCode::from(2);
            }
            _ => path = Some(argument),
        }
    }
    let Some(path) = path else {
        eprintln!("found no grammar file; expected its path\n\n{USAGE}");
        return ExitCode::from(2);
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let report = |diagnostics: &[anyfix_generator::Diagnostic]| {
        for diagnostic in diagnostics {
            eprintln!("{}\n", diagnostic.render(&source, &path));
        }
    };
    let generated = match anyfix_generator::generate(&source) {
        Ok(generated) => generated,
        Err(errors) => {
            report(&errors);
            return ExitCode::FAILURE;
        }
    };
    report(&generated.warnings);
    let code = match unformatted {
        true => generated.code,
        false => match anyfix_generator::format(&generated.code) {
            Ok(code) => code,
            Err(error) => {
                eprintln!(
                    "found no way to lay the output out ({error}); expected `rustfmt` to be installed; install it with `rustup component add rustfmt`, or pass --unformatted"
                );
                return ExitCode::FAILURE;
            }
        },
    };
    print!("{code}");
    ExitCode::SUCCESS
}
