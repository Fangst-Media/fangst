import pino from 'pino'

export const logger = pino({
  browser: {
    asObject: true,
    transmit: {
      level: 'warn',
      send: (level, logEvent) => {
        fetch('/api/logs', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            level,
            time: logEvent.ts,
            messages: logEvent.messages,
            bindings: logEvent.bindings,
          }),
        }).catch(() => {})
      },
    },
  },
  level: import.meta.env.DEV ? 'debug' : 'info',
})

export const createLogger = (component: string) => logger.child({ component })
