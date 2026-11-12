//! Turns the `#` comments of sources written before line strings into `##`
//! comments (see `anyx::migrate_comments`).
//!
//! ```sh
//! anyx-migrate-comments file.anyx …   # rewrites each file in place
//! anyx-migrate-comments < old > new   # or filters standard input
//! ```

use std::io::{Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        let mut source = String::new();
        if let Err(error) = std::io::stdin().read_to_string(&mut source) {
            eprintln!("anyx-migrate-comments: reading standard input: {error}");
            return ExitCode::FAILURE;
        }
        let migrated = anyx::migrate_comments(&source);
        return match std::io::stdout().write_all(migrated.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("anyx-migrate-comments: writing standard output: {error}");
                ExitCode::FAILURE
            }
        };
    }
    let failures = paths
        .iter()
        .filter(|path| {
            let result = std::fs::read_to_string(path)
                .and_then(|source| std::fs::write(path, anyx::migrate_comments(&source)));
            if let Err(error) = &result {
                eprintln!("anyx-migrate-comments: {path}: {error}");
            }
            result.is_err()
        })
        .count();
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
