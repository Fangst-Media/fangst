import { Hono } from 'hono'
import { configService } from '../services/config'
import type { User } from '../services/auth'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const metadata = new Hono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

// Proxy all requests to metadata-relay
// GET /api/metadata/tmdb/* -> metadata-relay/tmdb/*
metadata.all('/tmdb/*', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const relayUrl = configService.getMetadataRelayUrl()
  const path = c.req.path.replace('/api/metadata', '')
  const search = c.req.url.includes('?') ? `?${c.req.url.split('?')[1]}` : ''

  try {
    const response = await fetch(`${relayUrl}${path}${search}`, {
      method: c.req.method,
      headers: {
        'User-Agent': 'Jaeger/0.1.0',
        Accept: 'application/json',
      },
    })

    if (!response.ok) {
      const status = response.status as 400 | 401 | 403 | 404 | 500
      return c.json({ error: `Upstream error: ${response.statusText}` }, status)
    }

    const data = await response.json()
    return c.json(data)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Proxy request failed' },
      500
    )
  }
})

export default metadata
