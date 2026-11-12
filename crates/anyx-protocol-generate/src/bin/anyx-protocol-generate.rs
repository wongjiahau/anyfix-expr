//! `anyx-protocol-generate <rust|wasm|typescript> <schema.anyx>` writes the
//! code of a schema to standard output.

use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [target, path] = arguments.as_slice() else {
        eprintln!("usage: anyx-protocol-generate <rust|wasm|typescript> <schema.anyx>");
        return ExitCode::FAILURE;
    };
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let schema = match anyx_protocol::Schema::parse(&source) {
        Ok(schema) => schema,
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                eprintln!("{}", diagnostic.render(&source, path));
            }
            return ExitCode::FAILURE;
        }
    };
    let code = match target.as_str() {
        "rust" => anyx_protocol_generate::rust(&schema).map_err(|error| error.to_string()),
        "wasm" => Ok(anyx_protocol_generate::wasm(&schema)),
        "typescript" => {
            anyx_protocol_generate::typescript(&schema).map_err(|error| error.to_string())
        }
        other => Err(format!(
            "found the target `{other}`; expected `rust`, `wasm` or `typescript`"
        )),
    };
    let code = code.and_then(|code| match target.as_str() {
        "typescript" => Ok(code),
        _ => anyx_protocol_generate::format_rust(&code).map_err(|error| error.to_string()),
    });
    match code {
        Ok(code) => {
            print!("{code}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{path}: {message}");
            ExitCode::FAILURE
        }
    }
}
