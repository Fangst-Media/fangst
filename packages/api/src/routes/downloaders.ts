import { Hono } from 'hono'
import { configService } from '../services/config'
import { createDownloaderSchema, updateDownloaderSchema } from '@jaeger/shared/types/settings'
import { SABnzbdClient } from '@jaeger/shared/clients/sabnzbd'
import type { User } from '../services/auth'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const downloaders = new Hono<AuthEnv>()

function requireAdmin(user: User | null): user is User {
  return user !== null && user.role === 'admin'
}

// GET /api/downloaders - List all downloaders
downloaders.get('/', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  const items = configService.getDownloaders()

  // Mask API keys in response
  const masked = items.map((d) => ({
    ...d,
    apiKey: d.apiKey ? '********' : '',
  }))

  return c.json({ downloaders: masked })
})

// POST /api/downloaders - Add new downloader
downloaders.post('/', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const body = await c.req.json()
    const data = createDownloaderSchema.parse(body)
    const downloader = configService.addDownloader(data)

    return c.json({
      success: true,
      downloader: {
        ...downloader,
        apiKey: '********',
      },
    })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to add downloader' },
      400
    )
  }
})

// GET /api/downloaders/:id - Get specific downloader
downloaders.get('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  const id = c.req.param('id')
  const downloader = configService.getDownloader(id)

  if (!downloader) {
    return c.json({ error: 'Downloader not found' }, 404)
  }

  return c.json({
    downloader: {
      ...downloader,
      apiKey: '********',
    },
  })
})

// PATCH /api/downloaders/:id - Update downloader
downloaders.patch('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    const body = await c.req.json()
    const updates = updateDownloaderSchema.parse(body)
    const downloader = configService.updateDownloader(id, updates)

    return c.json({
      success: true,
      downloader: {
        ...downloader,
        apiKey: '********',
      },
    })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to update downloader' },
      400
    )
  }
})

// DELETE /api/downloaders/:id - Delete downloader
downloaders.delete('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    configService.deleteDownloader(id)

    return c.json({ success: true })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to delete downloader' },
      400
    )
  }
})

// POST /api/downloaders/:id/test - Test downloader connection
downloaders.post('/:id/test', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  const id = c.req.param('id')
  const downloader = configService.getDownloader(id)

  if (!downloader) {
    return c.json({ error: 'Downloader not found' }, 404)
  }

  if (downloader.type === 'sabnzbd') {
    const client = new SABnzbdClient(downloader.url, downloader.apiKey)
    const result = await client.testConnection()

    return c.json({
      success: result.success,
      version: result.version,
      error: result.error,
    })
  }

  return c.json({ error: 'Unknown downloader type' }, 400)
})

// POST /api/downloaders/test - Test connection with provided credentials (for add/edit form)
downloaders.post('/test', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const body = await c.req.json()
    const { type, url, apiKey } = body as { type: string; url: string; apiKey: string }

    if (!type || !url || !apiKey) {
      return c.json({ error: 'Missing required fields: type, url, apiKey' }, 400)
    }

    if (type === 'sabnzbd') {
      const client = new SABnzbdClient(url, apiKey)
      const result = await client.testConnection()

      return c.json({
        success: result.success,
        version: result.version,
        error: result.error,
      })
    }

    return c.json({ error: 'Unknown downloader type' }, 400)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to test connection' },
      400
    )
  }
})

export default downloaders
