//! A generated parser is copied into crates of any edition, so the generator's
//! output must build on edition 2021 too. This crate builds the generator's
//! test output on it. The files belong to anyx-generator's tests, which
//! format them, so they are not formatted again here.

#[rustfmt::skip]
#[path = "../../anyx-generator/tests/generated/config.rs"]
pub mod config;
#[rustfmt::skip]
#[path = "../../anyx-generator/tests/generated/corners.rs"]
pub mod corners;
#[rustfmt::skip]
#[path = "../../anyx-generator/tests/generated/json.rs"]
pub mod json;
#[rustfmt::skip]
#[path = "../../anyx-generator/tests/generated/language.rs"]
pub mod language;
