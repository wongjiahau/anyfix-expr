import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { defineConfig, type Plugin } from 'vite'
import { WebSocketServer } from 'ws'

/** The language server to run, built by `npm run dev`. */
const server =
  process.env.ANYX_LSP ??
  fileURLToPath(new URL('../../target/debug/examples/poentaro-lsp', import.meta.url))

/**
 * Bridges a WebSocket at `/lsp` to a language server on standard input and
 * output: one server process for each connection. The browser sends and
 * receives bare JSON messages; the server's side has `Content-Length` headers.
 */
function languageServer(): Plugin {
  return {
    name: 'anyx-language-server',
    configureServer(vite) {
      const sockets = new WebSocketServer({ noServer: true })
      vite.httpServer?.on('upgrade', (request, socket, head) => {
        if (request.url !== '/lsp') return
        sockets.handleUpgrade(request, socket, head, (websocket) => {
          const child = spawn(server, [], { stdio: ['pipe', 'pipe', 'inherit'] })
          let buffer = Buffer.alloc(0)
          child.stdout.on('data', (chunk: Buffer) => {
            buffer = Buffer.concat([buffer, chunk])
            for (;;) {
              const end = buffer.indexOf('\r\n\r\n')
              if (end < 0) return
              const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0, end).toString())?.[1])
              if (buffer.length < end + 4 + length) return
              websocket.send(buffer.subarray(end + 4, end + 4 + length).toString())
              buffer = buffer.subarray(end + 4 + length)
            }
          })
          websocket.on('message', (data) => {
            const body = Buffer.from(data.toString())
            child.stdin.write(`Content-Length: ${body.length}\r\n\r\n`)
            child.stdin.write(body)
          })
          websocket.on('close', () => child.kill())
          child.on('exit', () => websocket.close())
        })
      })
    },
  }
}

export default defineConfig({
  plugins: [languageServer()],
  // `../monaco/anyx-monaco.ts` lives outside this folder.
  server: { fs: { allow: ['..'] } },
})
