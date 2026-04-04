import { Hono } from 'hono'
import { setCookie, getCookie, deleteCookie } from 'hono/cookie'
import { z } from 'zod'
import { zValidator } from '@hono/zod-validator'
import { authService } from '../services/auth'
import { configService } from '../services/config'

const auth = new Hono()

const SESSION_COOKIE = 'jaeger_session'

const loginSchema = z.object({
  provider: z.enum(['jellyfin', 'plex', 'local']),
  username: z.string().min(1),
  password: z.string().default(''),
})

const registerSchema = z.object({
  username: z.string().min(1).max(50),
  password: z.string().min(8).max(128),
})

auth.post('/login', zValidator('json', loginSchema), async (c) => {
  const { provider, username, password } = c.req.valid('json')

  const ipAddress = c.req.header('x-forwarded-for') || c.req.header('x-real-ip') || 'unknown'
  const userAgent = c.req.header('user-agent')

  try {
    let result: Awaited<ReturnType<typeof authService.loginWithJellyfin>>

    if (provider === 'local') {
      result = await authService.loginWithLocal(username, password, ipAddress, userAgent)
    } else if (provider === 'jellyfin') {
      result = await authService.loginWithJellyfin(username, password, ipAddress, userAgent)
    } else {
      return c.json({ error: 'Plex authentication not yet implemented' }, 501)
    }

    setCookie(c, SESSION_COOKIE, result.session.id, {
      httpOnly: true,
      secure: process.env.NODE_ENV === 'production',
      sameSite: 'Lax',
      path: '/',
      expires: result.session.expiresAt,
    })

    // Compute effective request mode based on role and settings
    const settings = configService.get()
    const rolePermission =
      result.user.role === 'admin'
        ? settings.requests.adminRequestMode
        : settings.requests.memberRequestMode

    let effectiveRequestMode: 'simple' | 'detailed'
    if (rolePermission === 'user_choice') {
      effectiveRequestMode = result.user.preferredRequestMode ?? 'simple'
    } else {
      effectiveRequestMode = rolePermission === 'simple_only' ? 'simple' : 'detailed'
    }

    return c.json({
      user: {
        id: result.user.id,
        name: result.user.name,
        role: result.user.role,
        requestMode: effectiveRequestMode,
        canChooseRequestMode: rolePermission === 'user_choice',
      },
    })
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Authentication failed'
    return c.json({ error: message }, 401)
  }
})

auth.get('/me', async (c) => {
  const sessionId = getCookie(c, SESSION_COOKIE)

  if (!sessionId) {
    return c.json({ user: null })
  }

  const user = await authService.validateSession(sessionId)

  if (!user) {
    deleteCookie(c, SESSION_COOKIE)
    return c.json({ user: null })
  }

  // Compute effective request mode based on role and settings
  const settings = configService.get()
  const rolePermission =
    user.role === 'admin'
      ? settings.requests.adminRequestMode
      : settings.requests.memberRequestMode

  let effectiveRequestMode: 'simple' | 'detailed'
  if (rolePermission === 'user_choice') {
    effectiveRequestMode = user.preferredRequestMode ?? 'simple'
  } else {
    effectiveRequestMode = rolePermission === 'simple_only' ? 'simple' : 'detailed'
  }

  return c.json({
    user: {
      id: user.id,
      name: user.name,
      role: user.role,
      requestMode: effectiveRequestMode,
      canChooseRequestMode: rolePermission === 'user_choice',
    },
  })
})

auth.post('/logout', async (c) => {
  const sessionId = getCookie(c, SESSION_COOKIE)

  if (sessionId) {
    await authService.deleteSession(sessionId)
    deleteCookie(c, SESSION_COOKIE)
  }

  return c.json({ success: true })
})

auth.post('/register', zValidator('json', registerSchema), async (c) => {
  if (!configService.isLocalAuthEnabled() || !configService.isLocalRegistrationEnabled()) {
    return c.json({ error: 'Registration is disabled' }, 403)
  }

  const { username, password } = c.req.valid('json')
  const ipAddress = c.req.header('x-forwarded-for') || c.req.header('x-real-ip') || 'unknown'
  const userAgent = c.req.header('user-agent')

  try {
    const result = await authService.registerLocal(username, password, ipAddress, userAgent)

    setCookie(c, SESSION_COOKIE, result.session.id, {
      httpOnly: true,
      secure: process.env.NODE_ENV === 'production',
      sameSite: 'Lax',
      path: '/',
      expires: result.session.expiresAt,
    })

    const settings = configService.get()
    const rolePermission =
      result.user.role === 'admin'
        ? settings.requests.adminRequestMode
        : settings.requests.memberRequestMode

    let effectiveRequestMode: 'simple' | 'detailed'
    if (rolePermission === 'user_choice') {
      effectiveRequestMode = result.user.preferredRequestMode ?? 'simple'
    } else {
      effectiveRequestMode = rolePermission === 'simple_only' ? 'simple' : 'detailed'
    }

    return c.json({
      user: {
        id: result.user.id,
        name: result.user.name,
        role: result.user.role,
        requestMode: effectiveRequestMode,
        canChooseRequestMode: rolePermission === 'user_choice',
      },
    })
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Registration failed'
    return c.json({ error: message }, 400)
  }
})

auth.get('/providers', (c) => {
  const providers = authService.getEnabledProviders()
  const registrationEnabled = configService.isLocalAuthEnabled() && configService.isLocalRegistrationEnabled()
  return c.json({ providers, registrationEnabled })
})

export default auth
