import { OpenAPIHono } from '@hono/zod-openapi'
import { bookDetailsSchema } from '@jaeger/shared/schemas/metadata'
import { createDetailRoute, createLaneRoute, createSearchRoute } from '../lib/openapi-helpers'
import type { User } from '../services/auth'
import { configService } from '../services/config'
import { getMetadataService } from '../services/metadata'
import { MetadataStore } from '../services/metadata-store'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const books = new OpenAPIHono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

function getStore() {
  const relayUrl = configService.getMetadataRelayUrl()
  const tmdbApiKey = configService.getTmdbApiKey()
  const service = getMetadataService({ relayUrl, tmdbApiKey: tmdbApiKey || undefined })
  return new MetadataStore(service)
}

function laneHandler(fetchFn: (store: MetadataStore) => Promise<unknown[] | null>) {
  return async (c: any) => {
    const user = c.get('user')
    if (!requireAuth(user)) return c.json({ error: 'Authentication required' }, 401)
    try {
      const store = getStore()
      const results = await fetchFn(store)
      if (results === null) return c.json({ results: [], unsupported: true })
      return c.json({ results })
    } catch (error) {
      return c.json({ error: error instanceof Error ? error.message : 'Failed to fetch data' }, 500)
    }
  }
}

books.openapi(
  createLaneRoute('/trending', 'Get trending books', 'Books'),
  laneHandler((s) => s.getTrendingBooks())
)

books.openapi(
  createLaneRoute('/popular', 'Get popular books', 'Books'),
  laneHandler((s) => s.getPopularBooks())
)

books.openapi(
  createLaneRoute('/classics', 'Get classic books', 'Books'),
  laneHandler((s) => s.getClassicBooks())
)

function searchHandler(
  searchFn: (store: MetadataStore, query: string) => Promise<unknown[] | null>
) {
  return async (c: any) => {
    const user = c.get('user')
    if (!requireAuth(user)) return c.json({ error: 'Authentication required' }, 401)
    const { query } = c.req.valid('query')
    try {
      const store = getStore()
      const results = await searchFn(store, query)
      if (results === null) return c.json({ results: [], unsupported: true })
      return c.json({ results })
    } catch (error) {
      return c.json({ error: error instanceof Error ? error.message : 'Search failed' }, 500)
    }
  }
}

function detailHandler(fetchFn: (store: MetadataStore, id: string) => Promise<unknown | null>) {
  return async (c: any) => {
    const user = c.get('user')
    if (!requireAuth(user)) return c.json({ error: 'Authentication required' }, 401)
    const id = c.req.valid('param').workId ?? c.req.valid('param').id ?? ''
    try {
      const store = getStore()
      const details = await fetchFn(store, id)
      if (details === null) return c.json({ error: 'Not supported' }, 501)
      return c.json(details)
    } catch (error) {
      return c.json({ error: error instanceof Error ? error.message : 'Failed' }, 500)
    }
  }
}

books.openapi(
  createSearchRoute('/search', 'Search books', 'Books'),
  searchHandler((s, q) => s.searchBooks(q))
)

books.openapi(
  createDetailRoute('/{workId}', bookDetailsSchema, 'Get book details', 'Books', 'workId'),
  detailHandler((s, id) => s.getBookDetails(id))
)

export default books
