//! `anyx-protocol-generate <rust|wasm|typescript> <schema>` writes the code of
//! a schema to standard output. The schema is one `.anyx` file, or a folder
//! with `protocol.anyx` at its root and a file per scope under it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyx_protocol::{Schema, SchemaFile};

/// A file of the schema: its path relative to the schema's folder, and
/// what it says.
struct Read {
    path: String,
    source: String,
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [target, path] = arguments.as_slice() else {
        eprintln!(
            "usage: anyx-protocol-generate <rust|wasm|typescript> <schema.anyx | schema folder>"
        );
        return ExitCode::FAILURE;
    };
    let files = match read_schema(Path::new(path)) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("{path}: {message}");
            return ExitCode::FAILURE;
        }
    };
    let schema_files: Vec<SchemaFile> = files
        .iter()
        .map(|file| SchemaFile {
            path: &file.path,
            source: &file.source,
        })
        .collect();
    let schema = match Schema::parse_files(&schema_files) {
        Ok(schema) => schema,
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                let file = files.iter().find(|file| file.path == diagnostic.path);
                let source = file.map_or("", |file| file.source.as_str());
                let shown = match Path::new(path).is_dir() {
                    true => format!("{path}/{}", diagnostic.path),
                    false => path.clone(),
                };
                eprintln!("{}", diagnostic.render(source, &shown));
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

/// A file is the whole schema, as its root; a folder's `.anyx` files are its
/// scopes, by their paths in it.
fn read_schema(path: &Path) -> Result<Vec<Read>, String> {
    if !path.is_dir() {
        let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        return Ok(vec![Read {
            path: anyx_protocol::schema::ROOT_FILE.to_string(),
            source,
        }]);
    }
    let mut found = Vec::new();
    collect(path, &mut found)?;
    found.sort();
    found
        .into_iter()
        .map(|file| {
            let source = std::fs::read_to_string(&file)
                .map_err(|error| format!("{}: {error}", file.display()))?;
            let relative = file
                .strip_prefix(path)
                .expect("found under the folder")
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            Ok(Read {
                path: relative,
                source,
            })
        })
        .collect()
}

fn collect(folder: &Path, found: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect(&path, found)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "anyx")
        {
            found.push(path);
        }
    }
    Ok(())
}
