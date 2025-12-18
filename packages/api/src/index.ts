import { Hono } from 'hono'
import { cors } from 'hono/cors'
import { logger } from 'hono/logger'
import media from './routes/media'
import auth from './routes/auth'
import { authMiddleware } from './middleware/auth'

const app = new Hono()

app.use('*', logger())
app.use(
  '*',
  cors({
    origin: process.env.CORS_ORIGIN || 'http://localhost:5173',
    credentials: true,
  })
)
app.use('*', authMiddleware)

app.get('/', (c) => {
  return c.json({
    message: 'Jæger API',
    version: '0.1.0',
    mockMode: process.env.MOCK_MODE === 'true',
  })
})

app.get('/health', (c) => {
  return c.json({ status: 'ok' })
})

app.route('/api/auth', auth)
app.route('/api/media', media)

const port = process.env.PORT || 5001
console.log(`Server running on http://localhost:${port}`)
if (process.env.MOCK_FILESYSTEM === 'true') {
  console.log('[MOCK] Filesystem: loaded from mock-filesystem.json')
}

export default {
  port,
  fetch: app.fetch,
}
