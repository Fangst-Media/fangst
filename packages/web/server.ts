import { watch } from 'node:fs'
import type { ServerWebSocket } from 'bun'

const clients = new Set<ServerWebSocket<unknown>>()

Bun.serve({
  port: 5173,
  async fetch(req, server) {
    const url = new URL(req.url)

    if (url.pathname === '/hot') {
      const success = server.upgrade(req)
      return success ? undefined : new Response('WebSocket upgrade failed', { status: 500 })
    }

    if (url.pathname.startsWith('/api')) {
      const apiUrl = `http://localhost:5001${url.pathname}${url.search}`
      return fetch(apiUrl, {
        method: req.method,
        headers: req.headers,
        body: req.body,
      })
    }

    if (url.pathname.endsWith('.tsx') || url.pathname.endsWith('.ts')) {
      const filePath = `.${url.pathname}`
      const file = Bun.file(filePath)
      if (await file.exists()) {
        const transpiled = await Bun.build({
          entrypoints: [filePath],
          format: 'esm',
        })
        const output = await transpiled.outputs[0]?.text()
        return new Response(output, {
          headers: { 'Content-Type': 'application/javascript' },
        })
      }
    }

    if (url.pathname === '/' || !url.pathname.includes('.')) {
      return new Response(Bun.file('./index.html'), {
        headers: { 'Content-Type': 'text/html' },
      })
    }

    const filePath = `.${url.pathname}`
    const file = Bun.file(filePath)

    if (await file.exists()) {
      return new Response(file)
    }

    return new Response('Not found', { status: 404 })
  },
  websocket: {
    open(ws) {
      clients.add(ws)
    },
    close(ws) {
      clients.delete(ws)
    },
    message() {},
  },
})

console.log('🎨 Frontend dev server running on http://localhost:5173')

watch('./src', { recursive: true }, (eventType, filename) => {
  if (filename) {
    console.log(`📝 File changed: ${filename}`)
    for (const client of clients) {
      client.send('reload')
    }
  }
})
