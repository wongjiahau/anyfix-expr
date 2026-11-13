//! The server in a browser, as WebAssembly: a page's Web Worker hands each
//! message to [`LanguageServer::handle`] and posts back what it returns.
//! [`wasm!`](crate::wasm!) exports one for a language.

use ::wasm_bindgen::prelude::wasm_bindgen;
use lsp_server::Message;

#[doc(hidden)]
pub use ::wasm_bindgen;

use crate::{Language, Server};

/// A language server that takes and gives JSON-RPC messages as text, without
/// the `Content-Length` headers standard input and output carry.
#[wasm_bindgen]
pub struct LanguageServer {
    server: Server<'static>,
}

impl LanguageServer {
    pub fn new(language: &'static dyn Language) -> Self {
        LanguageServer {
            server: Server::new(language),
        }
    }
}

#[wasm_bindgen]
impl LanguageServer {
    /// The messages to send back for `message`. A message that isn't
    /// JSON-RPC gets none.
    pub fn handle(&mut self, message: &str) -> Vec<String> {
        let Ok(message) = serde_json::from_str::<Message>(message) else {
            return Vec::new();
        };
        self.server
            .handle(message)
            .into_iter()
            .filter_map(|reply| {
                let mut reply = serde_json::to_value(reply).ok()?;
                reply
                    .as_object_mut()?
                    .insert("jsonrpc".into(), "2.0".into());
                Some(reply.to_string())
            })
            .collect()
    }
}

/// Exports `languageServer()` to JavaScript, which starts a
/// [`LanguageServer`] for the [`Language`] `$name`. Build the crate it is in
/// as a `cdylib` with `wasm-pack build --target web`.
///
/// ```ignore
/// anyx_lsp::language!(MyLanguage = generated);
/// anyx_lsp::wasm!(MyLanguage);
/// ```
// rustfmt indents the attribute further on every run, so it is laid out here.
#[rustfmt::skip]
#[macro_export]
macro_rules! wasm {
    ($name:ident) => {
        #[$crate::wasm::wasm_bindgen::prelude::wasm_bindgen(
            js_name = languageServer,
            wasm_bindgen = $crate::wasm::wasm_bindgen
        )]
        pub fn language_server() -> $crate::wasm::LanguageServer {
            $crate::wasm::LanguageServer::new(&$name)
        }
    };
}
