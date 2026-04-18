import { OpenAPIHono } from '@hono/zod-openapi'
import { searchService } from '../services/search'
import { configService } from '../services/config'
import { searchRequestSchema } from '@jaeger/shared/types/indexer'
import { SABnzbdClient } from '@jaeger/shared/clients/sabnzbd'
import { requestService } from '../services/requests'
import { downloadService } from '../services/downloads'
import type { User } from '../services/auth'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const search = new OpenAPIHono<AuthEnv>()

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
    const { downloadUrl, downloaderId, releaseName, category } = body as {
      downloadUrl: string
      downloaderId: string
      releaseName: string
      category?: string
    }

    if (!downloadUrl || !downloaderId || !releaseName) {
      return c.json({ error: 'Missing required fields: downloadUrl, downloaderId, releaseName' }, 400)
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
      const result = await client.addNzbFromUrl(downloadUrl, releaseName, category)

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

// POST /api/search/download-release - Download a specific release and create request
// Now a wrapper: creates request (manual mode) + creates download via DownloadService
search.post('/download-release', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const body = await c.req.json()
    const {
      downloadUrl,
      downloaderId,
      releaseName,
      category,
      media,
    } = body as {
      downloadUrl: string
      downloaderId: string
      releaseName: string
      category?: string
      media: {
        title: string
        year?: number
        type: 'movie' | 'tv'
        tmdbId?: string
        imdbId?: string
        posterPath?: string
        overview?: string
        season?: number
      }
    }

    if (!downloadUrl || !downloaderId || !releaseName || !media) {
      return c.json(
        { error: 'Missing required fields: downloadUrl, downloaderId, releaseName, media' },
        400
      )
    }

    if (!media.title) {
      return c.json({ error: 'Missing media.title' }, 400)
    }

    const downloader = configService.getDownloader(downloaderId)
    if (!downloader) {
      return c.json({ error: 'Downloader not found' }, 404)
    }

    if (!downloader.enabled) {
      return c.json({ error: 'Downloader is disabled' }, 400)
    }

    // Create request in manual mode
    const isAdmin = user.role === 'admin'
    const request = await requestService.create({ ...media, mode: 'manual' }, user.id, isAdmin)

    // Create download via DownloadService
    const download = await downloadService.createForRequest(request.id, {
      releaseName,
      downloadUrl,
    })

    const result = await downloadService.sendToDownloader(download.id, downloadUrl, category)

    // Update request status to downloading
    await requestService.updateStatus(request.id, 'downloading')

    return c.json({
      success: true,
      nzoId: result.nzoId,
      request: await requestService.getById(request.id),
      download: await downloadService.getById(download.id),
    })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to download release' },
      400
    )
  }
})

export default search
