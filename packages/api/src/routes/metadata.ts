import type { MetadataRelayResult } from '@jaeger/shared/clients/metadata-relay'
import { MetadataRelayClient } from '@jaeger/shared/clients/metadata-relay'
import { Hono } from 'hono'
import type { User } from '../services/auth'
import { configService } from '../services/config'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const metadata = new Hono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

// Simple in-memory cache for trending data (refreshes every 10 minutes)
const cache = new Map<string, { data: MetadataRelayResult[]; expires: number }>()
const CACHE_TTL = 10 * 60 * 1000 // 10 minutes

function getCached(key: string): MetadataRelayResult[] | null {
  const entry = cache.get(key)
  if (!entry) return null
  if (Date.now() > entry.expires) {
    cache.delete(key)
    return null
  }
  return entry.data
}

function setCache(key: string, data: MetadataRelayResult[]): void {
  cache.set(key, { data, expires: Date.now() + CACHE_TTL })
}

// Trending movies with caching
metadata.get('/tmdb/movies/trending', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const cached = getCached('trending-movies')
    if (cached) {
      return c.json({ results: cached })
    }

    const relayUrl = configService.getMetadataRelayUrl()
    const client = new MetadataRelayClient(relayUrl)
    const results = await client.getTrendingMovies()
    setCache('trending-movies', results)
    return c.json({ results })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch trending movies' },
      500
    )
  }
})

// Trending TV shows with caching - MUST be before /tmdb/tv/:id
metadata.get('/tmdb/tv/trending', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const cached = getCached('trending-tv')
    if (cached) {
      return c.json({ results: cached })
    }

    const relayUrl = configService.getMetadataRelayUrl()
    const client = new MetadataRelayClient(relayUrl)
    const results = await client.getTrendingTv()
    setCache('trending-tv', results)
    return c.json({ results })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch trending TV' },
      500
    )
  }
})

// Get TV series details with seasons info
metadata.get('/tmdb/tv/:id', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const tmdbId = c.req.param('id')

  try {
    const relayUrl = configService.getMetadataRelayUrl()
    const client = new MetadataRelayClient(relayUrl)
    const details = await client.getTvDetails(tmdbId)
    return c.json(details)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch TV details' },
      500
    )
  }
})

// Proxy all other requests to metadata-relay
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
    return c.json({ error: error instanceof Error ? error.message : 'Proxy request failed' }, 500)
  }
})

export default metadata
