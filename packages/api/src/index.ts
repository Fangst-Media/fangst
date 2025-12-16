import { Hono } from 'hono'
import { cors } from 'hono/cors'
import { logger } from 'hono/logger'

const app = new Hono()

app.use('*', logger())
app.use('*', cors({
  origin: process.env.CORS_ORIGIN || 'http://localhost:5173',
  credentials: true,
}))

app.get('/', (c) => {
  return c.json({
    message: 'Jæger API',
    version: '0.1.0',
  })
})

app.get('/health', (c) => {
  return c.json({ status: 'ok' })
})

const port = process.env.PORT || 5001
console.log(`🚀 Server running on http://localhost:${port}`)

export default {
  port,
  fetch: app.fetch,
}
