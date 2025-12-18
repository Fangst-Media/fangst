import { createMiddleware } from 'hono/factory'
import { getCookie } from 'hono/cookie'
import { authService, type User, type UserRole } from '../services/auth'

const SESSION_COOKIE = 'jaeger_session'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

export const authMiddleware = createMiddleware<AuthEnv>(async (c, next) => {
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
