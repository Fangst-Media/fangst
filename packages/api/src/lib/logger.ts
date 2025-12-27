import pino from 'pino'
import { createStream } from 'rotating-file-stream'
import { mkdir } from 'node:fs/promises'

// Ensure logs directory exists
const logsDir = './logs'
await mkdir(logsDir, { recursive: true }).catch(() => {})

// Rotating file stream for production
const fileStream = createStream('app.log', {
  path: logsDir,
  size: '10M',
  interval: '1d',
  compress: 'gzip',
  maxFiles: 7,
})

const isProduction = process.env.NODE_ENV === 'production'
const logLevel = process.env.LOG_LEVEL || 'info'
const loggingEnabled = process.env.LOGGING_ENABLED !== 'false'

function getTransport(): pino.TransportSingleOptions | undefined {
  if (!isProduction) {
    return {
      target: 'pino-pretty',
      options: {
        colorize: true,
        translateTime: 'HH:MM:ss',
        ignore: 'pid,hostname',
      },
    }
  }

  // Production with logging disabled: JSON to stdout only
  if (!loggingEnabled) {
    return undefined
  }

  // Production with logging enabled: multi-destination (stdout + file)
  return {
    target: 'pino/file',
    options: { destination: 1 }, // stdout, file handled separately
  }
}

// Create base logger
export const logger = pino(
  {
    level: logLevel,
    transport: getTransport(),
    base: { app: 'jaeger' },
    redact: ['apiKey', 'password', '*.apiKey', '*.password'],
  },
  // In production with logging enabled, write to both stdout and file
  isProduction && loggingEnabled
    ? pino.multistream([
        { stream: process.stdout },
        { stream: fileStream },
      ])
    : undefined
)

export const createServiceLogger = (service: string) => logger.child({ service })
