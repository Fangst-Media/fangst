import { Hono } from 'hono'
import { configService } from '../services/config'
import { createIndexerSchema, updateIndexerSchema } from '@jaeger/shared/types/indexer'
import { NewznabClient } from '@jaeger/shared/clients/newznab'
import { getIndexerList } from '@jaeger/shared/data/indexer-registry'
import type { User } from '../services/auth'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const indexers = new Hono<AuthEnv>()

function requireAdmin(user: User | null): user is User {
  return user !== null && user.role === 'admin'
}

// GET /api/indexers/registry - Get list of known indexers
indexers.get('/registry', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  return c.json({ indexers: getIndexerList() })
})

// GET /api/indexers - List all indexers
indexers.get('/', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  const items = configService.getIndexers()

  // Mask API keys in response
  const masked = items.map((i) => ({
    ...i,
    apiKey: i.apiKey ? '********' : '',
  }))

  return c.json({ indexers: masked })
})

// POST /api/indexers - Add new indexer
indexers.post('/', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const body = await c.req.json()
    const data = createIndexerSchema.parse(body)
    const indexer = configService.addIndexer(data)

    return c.json({
      success: true,
      indexer: {
        ...indexer,
        apiKey: '********',
      },
    })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to add indexer' },
      400
    )
  }
})

// GET /api/indexers/:id - Get specific indexer
indexers.get('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  const id = c.req.param('id')
  const indexer = configService.getIndexer(id)

  if (!indexer) {
    return c.json({ error: 'Indexer not found' }, 404)
  }

  return c.json({
    indexer: {
      ...indexer,
      apiKey: '********',
    },
  })
})

// PATCH /api/indexers/:id - Update indexer
indexers.patch('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    const body = await c.req.json()
    const updates = updateIndexerSchema.parse(body)
    const indexer = configService.updateIndexer(id, updates)

    return c.json({
      success: true,
      indexer: {
        ...indexer,
        apiKey: '********',
      },
    })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to update indexer' },
      400
    )
  }
})

// DELETE /api/indexers/:id - Delete indexer
indexers.delete('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    configService.deleteIndexer(id)

    return c.json({ success: true })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to delete indexer' },
      400
    )
  }
})

// POST /api/indexers/:id/test - Test indexer connection
indexers.post('/:id/test', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  const id = c.req.param('id')
  const indexer = configService.getIndexer(id)

  if (!indexer) {
    return c.json({ error: 'Indexer not found' }, 404)
  }

  if (indexer.protocol === 'newznab') {
    const client = new NewznabClient(indexer.url, indexer.apiKey)
    const result = await client.testConnection()

    return c.json({
      success: result.success,
      version: result.version,
      error: result.error,
    })
  }

  return c.json({ error: 'Unknown indexer protocol' }, 400)
})

// POST /api/indexers/test - Test connection with provided credentials (for add/edit form)
indexers.post('/test', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const body = await c.req.json()
    const { protocol, url, apiKey } = body as { protocol: string; url: string; apiKey: string }

    if (!protocol || !url || !apiKey) {
      return c.json({ error: 'Missing required fields: protocol, url, apiKey' }, 400)
    }

    if (protocol === 'newznab') {
      const client = new NewznabClient(url, apiKey)
      const result = await client.testConnection()

      return c.json({
        success: result.success,
        version: result.version,
        error: result.error,
      })
    }

    return c.json({ error: 'Unknown indexer protocol' }, 400)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to test connection' },
      400
    )
  }
})

export default indexers
