/**
 * The language server, as WebAssembly in a Web Worker. The page posts each
 * JSON-RPC message here, and gets back what the server answers.
 */

import init, { languageServer } from './pkg/poentaro_lsp.js'

// Messages that arrive while the WebAssembly loads wait for it.
const waiting: string[] = []
let handle = (message: string) => void waiting.push(message)

self.addEventListener('message', (event: MessageEvent<string>) => handle(event.data))

await init()
const server = languageServer()
handle = (message) => {
  for (const reply of server.handle(message)) self.postMessage(reply)
}
waiting.splice(0).forEach(handle)
