import { Hono } from 'hono'
import { searchService } from '../services/search'
import { configService } from '../services/config'
import { searchRequestSchema } from '@jaeger/shared/types/indexer'
import { SABnzbdClient } from '@jaeger/shared/clients/sabnzbd'
import type { User } from '../services/auth'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const search = new Hono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

// POST /api/search - Search all enabled indexers
search.post('/', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const body = await c.req.json()
    const request = searchRequestSchema.parse(body)
    const response = await searchService.search(request)

    return c.json(response)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Search failed' },
      400
    )
  }
})

// POST /api/search/download - Send NZB to downloader
search.post('/download', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const body = await c.req.json()
    const { downloadUrl, downloaderId, category } = body as {
      downloadUrl: string
      downloaderId: string
      category?: string
    }

    if (!downloadUrl || !downloaderId) {
      return c.json({ error: 'Missing required fields: downloadUrl, downloaderId' }, 400)
    }

    const downloader = configService.getDownloader(downloaderId)
    if (!downloader) {
      return c.json({ error: 'Downloader not found' }, 404)
    }

    if (!downloader.enabled) {
      return c.json({ error: 'Downloader is disabled' }, 400)
    }

    if (downloader.type === 'sabnzbd') {
      const client = new SABnzbdClient(downloader.url, downloader.apiKey)
      const result = await client.addNzb(downloadUrl, category)

      return c.json({
        success: true,
        nzoId: result.nzoId,
      })
    }

    return c.json({ error: 'Unknown downloader type' }, 400)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to send to downloader' },
      400
    )
  }
})

export default search
