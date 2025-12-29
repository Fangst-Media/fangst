import { Hono } from 'hono'
import { cors } from 'hono/cors'
import activity from './routes/activity'
import auth from './routes/auth'
import calendarRouter from './routes/calendar'
import downloaders from './routes/downloaders'
import indexers from './routes/indexers'
import libraryRouter from './routes/library'
import logsRouter from './routes/logs'
import media from './routes/media'
import metadata from './routes/metadata'
import requestsRouter from './routes/requests'
import search from './routes/search'
import settings from './routes/settings'
import usersRouter from './routes/users'
import { authMiddleware } from './middleware/auth'
import { requestLogger } from './middleware/logging'
import { configService } from './services/config'
import { logger } from './lib/logger'
import { startScheduler } from './lib/scheduler'

// Initialize config before starting server
await configService.initialize()

const app = new Hono()

// Request logging (before auth so we log all requests)
app.use('*', requestLogger)

app.use(
  '*',
  cors({
    origin: configService.getCorsOrigin(),
    credentials: true,
  })
)
app.use('*', authMiddleware)

app.get('/', (c) => {
  return c.json({
    message: 'Jaeger API',
    version: '0.1.0',
    mockFilesystem: configService.isMockFilesystem(),
    mockAuth: configService.isMockAuth(),
  })
})

app.get('/health', (c) => {
  return c.json({ status: 'ok' })
})

app.route('/api/auth', auth)
app.route('/api/logs', logsRouter)
app.route('/api/media', media)
app.route('/api/library', libraryRouter)
app.route('/api/settings', settings)
app.route('/api/downloaders', downloaders)
app.route('/api/indexers', indexers)
app.route('/api/search', search)
app.route('/api/metadata', metadata)
app.route('/api/activity', activity)
app.route('/api/calendar', calendarRouter)
app.route('/api/requests', requestsRouter)
app.route('/api/users', usersRouter)

const port = configService.getPort()
logger.info({ port }, 'Server starting')
if (configService.isMockFilesystem()) {
  logger.info('[MOCK] Filesystem: loaded from mock-filesystem.json')
}
if (configService.isMockAuth()) {
  logger.info('[MOCK] Auth: enabled')
}

// Start the job scheduler
startScheduler()

export default {
  port,
  fetch: app.fetch,
}
