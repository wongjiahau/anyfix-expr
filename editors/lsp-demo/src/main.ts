/**
 * Monaco, talking to an anyx language server that runs as WebAssembly in a
 * Web Worker (see worker.ts), so the page needs no server. The client is
 * small on purpose: it speaks just enough of the protocol for diagnostics,
 * completion (with the grammar's `#` documentation), formatting and folding.
 */

import * as monaco from 'monaco-editor'
import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker'
import { registerAnyx } from '../../monaco/anyx-monaco'

self.MonacoEnvironment = { getWorker: () => new editorWorker() }

const languageId = 'poentaro'
const uri = monaco.Uri.parse('file:///demo.poentaro')
const status = document.getElementById('status')!

const sample = `metadata {
    title "Minuets in G Major and G Minor",
    subtitle "Christian Petzold (BWV Anh. 114 & 115)",
    max measures per system 4,
},

## \`chord\` is a mistake: the grammar wants \`chords\`.
parts [
    "Pluck (Guzheng, Liuqin, etc)" [p]: notes { sound 6 },
    "Sustained (Violin, Harmonica, etc)" [s]: notes { sound 21, volume 49 % },
    "Chord (Piano, Guitar, etc)" [c]: chord { sound 0 },
],

score """
    key=G4 bpm=100 time=3/4 label="A"
    [p] 5 1_ 2_ 3_ 4_
    [s] 1,- 2,
    [c] 1-2m
    """,
`

registerAnyx(monaco, { languageId, extensions: ['.poentaro'] })
const model = monaco.editor.createModel(sample, languageId, uri)
const editor = monaco.editor.create(document.getElementById('editor')!, {
  model,
  theme: 'anyx-dark',
  automaticLayout: true,
  fontSize: 14,
  minimap: { enabled: false },
})

// ── JSON-RPC with the worker ───────────────────────────────────────────────

type Position = { line: number; character: number }
type Range = { start: Position; end: Position }

const server = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
const pending = new Map<number, (result: unknown) => void>()
let nextId = 1

function send(message: object) {
  server.postMessage(JSON.stringify({ jsonrpc: '2.0', ...message }))
}

// Resolves once the server has answered `initialize` and been told
// `initialized`; until then it takes no other request.
let markReady = () => {}
const ready = new Promise<void>((resolve) => (markReady = resolve))

function request<T>(method: string, params: unknown): Promise<T> {
  const id = nextId++
  send({ id, method, params })
  return new Promise((resolve) => pending.set(id, resolve as (result: unknown) => void))
}

function notify(method: string, params: unknown) {
  send({ method, params })
}

const toRange = (range: Range): monaco.IRange => ({
  startLineNumber: range.start.line + 1,
  startColumn: range.start.character + 1,
  endLineNumber: range.end.line + 1,
  endColumn: range.end.character + 1,
})

const textDocument = { uri: uri.toString() }

server.addEventListener('message', (event: MessageEvent<string>) => {
  const message = JSON.parse(event.data)
  if ('id' in message && pending.has(message.id)) {
    pending.get(message.id)!(message.result)
    pending.delete(message.id)
  } else if (message.method === 'textDocument/publishDiagnostics') {
    const diagnostics: { range: Range; message: string; source?: string }[] = message.params.diagnostics
    monaco.editor.setModelMarkers(
      model,
      'anyx-lsp',
      diagnostics.map((diagnostic) => ({
        ...toRange(diagnostic.range),
        message: diagnostic.message,
        source: diagnostic.source,
        severity: monaco.MarkerSeverity.Error,
      })),
    )
    status.textContent =
      diagnostics.length === 0 ? 'no problems' : `${diagnostics.length} problem${diagnostics.length === 1 ? '' : 's'}`
  }
})

server.addEventListener('error', () => (status.textContent = 'the language server failed to start'))

;(async () => {
  await request('initialize', { processId: null, rootUri: null, capabilities: {} })
  notify('initialized', {})
  markReady()
  let version = 1
  notify('textDocument/didOpen', {
    textDocument: { ...textDocument, languageId, version, text: model.getValue() },
  })
  model.onDidChangeContent(() => {
    version += 1
    notify('textDocument/didChange', {
      textDocument: { ...textDocument, version },
      contentChanges: [{ text: model.getValue() }],
    })
  })
})()

// ── Monaco providers, each one request to the server ───────────────────────

monaco.languages.registerCompletionItemProvider(languageId, {
  triggerCharacters: [' ', '[', '{', '('],
  async provideCompletionItems(model, position) {
    await ready
    const items: {
      label: string
      detail?: string
      documentation?: { kind: string; value: string }
      insertText: string
    }[] = await request(
      'textDocument/completion',
      { textDocument, position: { line: position.lineNumber - 1, character: position.column - 1 } },
    )
    const word = model.getWordUntilPosition(position)
    const range = new monaco.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn)
    return {
      suggestions: items.map((item) => ({
        label: item.label,
        detail: item.detail,
        documentation: item.documentation && { value: item.documentation.value },
        kind: monaco.languages.CompletionItemKind.Keyword,
        insertText: item.insertText,
        insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
        range,
      })),
    }
  },
})

monaco.languages.registerDocumentFormattingEditProvider(languageId, {
  async provideDocumentFormattingEdits() {
    await ready
    const edits: { range: Range; newText: string }[] | null = await request('textDocument/formatting', {
      textDocument,
      options: { tabSize: 4, insertSpaces: true },
    })
    return (edits ?? []).map((edit) => ({ range: toRange(edit.range), text: edit.newText }))
  },
})

monaco.languages.registerFoldingRangeProvider(languageId, {
  async provideFoldingRanges() {
    await ready
    const ranges: { startLine: number; endLine: number }[] = await request('textDocument/foldingRange', {
      textDocument,
    })
    return ranges.map((range) => ({ start: range.startLine + 1, end: range.endLine + 1 }))
  },
})

editor.focus()
