import { defineConfig } from 'vite'

export default defineConfig({
  // Relative asset paths, so the built page works from any folder of any
  // static host.
  base: './',
  // The language server's worker loads its WebAssembly with top-level await.
  worker: { format: 'es' },
  build: { target: 'es2022' },
  // `../monaco/anyx-monaco.ts` lives outside this folder.
  server: { fs: { allow: ['..'] } },
})
