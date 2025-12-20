import { Hono } from 'hono'
import { cors } from 'hono/cors'
import { logger } from 'hono/logger'
import media from './routes/media'
import auth from './routes/auth'
import settings from './routes/settings'
import downloaders from './routes/downloaders'
import indexers from './routes/indexers'
import search from './routes/search'
import metadata from './routes/metadata'
import activity from './routes/activity'
import { authMiddleware } from './middleware/auth'
import { configService } from './services/config'

// Initialize config before starting server
await configService.initialize()

const app = new Hono()

app.use('*', logger())
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
app.route('/api/media', media)
app.route('/api/settings', settings)
app.route('/api/downloaders', downloaders)
app.route('/api/indexers', indexers)
app.route('/api/search', search)
app.route('/api/metadata', metadata)
app.route('/api/activity', activity)

const port = configService.getPort()
console.log(`Server running on http://localhost:${port}`)
if (configService.isMockFilesystem()) {
  console.log('[MOCK] Filesystem: loaded from mock-filesystem.json')
}
if (configService.isMockAuth()) {
  console.log('[MOCK] Auth: enabled')
}

export default {
  port,
  fetch: app.fetch,
}
