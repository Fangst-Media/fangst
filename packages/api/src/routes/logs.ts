import { Hono } from 'hono'
import { createServiceLogger } from '../lib/logger'

const logsRouter = new Hono()
const log = createServiceLogger('frontend')

type LogLevel = 'debug' | 'info' | 'warn' | 'error'

logsRouter.post('/', async (c) => {
  try {
    const body = await c.req.json()
    const { level, messages, bindings } = body as {
      level: LogLevel
      messages: string | string[]
      bindings?: Record<string, unknown>
    }

    const message = Array.isArray(messages) ? messages.join(' ') : String(messages)
    const logFn = log[level] || log.info

    logFn.call(log, { ...bindings, source: 'browser' }, `[FE] ${message}`)

    return c.json({ ok: true })
  } catch {
    return c.json({ error: 'Invalid log payload' }, 400)
  }
})

export default logsRouter
