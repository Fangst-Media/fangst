import { createMiddleware } from 'hono/factory'
import type { Logger } from 'pino'
import { logger } from '../lib/logger'

type LoggingEnv = {
  Variables: {
    log: Logger
    requestId: string
  }
}

export const requestLogger = createMiddleware<LoggingEnv>(async (c, next) => {
  const requestId = crypto.randomUUID().slice(0, 8)
  const start = Date.now()

  const log = logger.child({
    requestId,
    method: c.req.method,
    path: c.req.path,
  })

  c.set('log', log)
  c.set('requestId', requestId)

  await next()

  const duration = Date.now() - start
  const status = c.res.status

  if (status >= 500) {
    log.error({ status, duration }, 'Request failed')
  } else if (status >= 400) {
    log.warn({ status, duration }, 'Request error')
  } else {
    log.info({ status, duration }, 'Request completed')
  }
})
