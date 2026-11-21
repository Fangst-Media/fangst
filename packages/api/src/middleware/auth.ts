import { getCookie } from 'hono/cookie'
import { createMiddleware } from 'hono/factory'
import { type User, type UserRole, authService } from '../services/auth'
import { configService } from '../services/config'

const SESSION_COOKIE = 'jaeger_session'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

// Virtual admin user for API key authentication
const API_KEY_USER: User = {
  id: 'api-key',
  providerId: 'api-key',
  provider: 'jellyfin',
  name: 'API Key',
  role: 'admin',
  preferredRequestMode: 'detailed',
  createdAt: new Date(),
  lastSeenAt: new Date(),
}

export const authMiddleware = createMiddleware<AuthEnv>(async (c, next) => {
  // Check for API key in header (X-Api-Key) or query param (?apiKey=)
  const apiKeyHeader = c.req.header('X-Api-Key')
  const apiKeyQuery = c.req.query('apiKey')
  const providedApiKey = apiKeyHeader || apiKeyQuery

  if (providedApiKey) {
    const validApiKey = configService.getApiKey()
    if (providedApiKey === validApiKey) {
      c.set('user', API_KEY_USER)
      await next()
      return
    }
  }

  // Fall back to session-based auth
  const sessionId = getCookie(c, SESSION_COOKIE)

  if (sessionId) {
    const user = await authService.validateSession(sessionId)
    c.set('user', user)
  } else {
    c.set('user', null)
  }

  await next()
})

export const requireAuth = createMiddleware<AuthEnv>(async (c, next) => {
  const user = c.get('user')

  if (!user) {
    return c.json({ error: 'Unauthorized' }, 401)
  }

  return next()
})

export const requireRole = (role: UserRole) =>
  createMiddleware<AuthEnv>(async (c, next) => {
    const user = c.get('user')

    if (!user) {
      return c.json({ error: 'Unauthorized' }, 401)
    }

    if (role === 'admin' && user.role !== 'admin') {
      return c.json({ error: 'Forbidden' }, 403)
    }

    return next()
  })
