# anyx language server demo

Monaco editing a `.poentaro` score, with diagnostics, completion, formatting
and folding from poentaro's grammar alone. The language server is
[`anyx-lsp`](../../crates/anyx-lsp) compiled to WebAssembly, running in a Web
Worker, so the page is static: no server behind it.

```sh
npm install
npm run dev     # builds the WebAssembly, then serves the page with Vite
npm run build   # writes the static page to dist/, for any static host
```

Both need [`wasm-pack`](https://rustwasm.github.io/wasm-pack/).

## For your own grammar

1. A `cdylib` crate with the decoder anyx-generate wrote, like [`wasm/`](wasm):

   ```rust
   mod generated;
   anyx_lsp::language!(MyLanguage = generated);
   anyx_lsp::wasm!(MyLanguage);
   ```

   with `anyx-lsp = { …, features = ["wasm"] }`.
2. `wasm-pack build --target web`.
3. A worker that loads it and passes messages through, like
   [`src/worker.ts`](src/worker.ts), and an LSP client talking to that
   worker, like [`src/main.ts`](src/main.ts).
