import { OpenAPIHono } from '@hono/zod-openapi'
import { SABnzbdClient } from '@fangst/shared/clients/sabnzbd'
import type { User } from '../services/auth'
import { configService } from '../services/config'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const activity = new OpenAPIHono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

// GET /api/activity - Get download queue and recent history from all downloaders
activity.get('/', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const downloaders = configService.getEnabledDownloaders()

  if (downloaders.length === 0) {
    return c.json({
      queue: [],
      history: [],
      stats: null,
    })
  }

  const results = await Promise.all(
    downloaders.map(async (downloader) => {
      if (downloader.type === 'sabnzbd') {
        const client = new SABnzbdClient(downloader.url, downloader.apiKey)
        try {
          const [queue, history] = await Promise.all([client.getQueue(), client.getHistory(10)])
          return {
            downloader: downloader.name,
            downloaderId: downloader.id,
            queue: queue.slots.map((slot) => ({
              ...slot,
              downloader: downloader.name,
              downloaderId: downloader.id,
            })),
            history: history.slots.map((slot) => ({
              ...slot,
              downloader: downloader.name,
              downloaderId: downloader.id,
            })),
            stats: {
              status: queue.status,
              speed: queue.speed,
              sizeLeft: queue.sizeLeft,
              timeLeft: queue.timeLeft,
              daySize: history.daySize,
              weekSize: history.weekSize,
              monthSize: history.monthSize,
              totalSize: history.totalSize,
            },
          }
        } catch (error) {
          return {
            downloader: downloader.name,
            downloaderId: downloader.id,
            error: error instanceof Error ? error.message : 'Failed to fetch activity',
            queue: [],
            history: [],
            stats: null,
          }
        }
      }
      return {
        downloader: downloader.name,
        downloaderId: downloader.id,
        error: 'Unsupported downloader type',
        queue: [],
        history: [],
        stats: null,
      }
    })
  )

  // Combine results from all downloaders
  const combined = {
    queue: results.flatMap((r) => r.queue),
    history: results.flatMap((r) => r.history),
    stats: results.find((r) => r.stats)?.stats ?? null,
    errors: results
      .filter((r) => r.error)
      .map((r) => ({
        downloader: r.downloader,
        error: r.error,
      })),
  }

  return c.json(combined)
})

// POST /api/activity/pause - Pause all downloaders
activity.post('/pause', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const downloaders = configService.getEnabledDownloaders()

  for (const downloader of downloaders) {
    if (downloader.type === 'sabnzbd') {
      const client = new SABnzbdClient(downloader.url, downloader.apiKey)
      await client.pause()
    }
  }

  return c.json({ success: true })
})

// POST /api/activity/resume - Resume all downloaders
activity.post('/resume', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const downloaders = configService.getEnabledDownloaders()

  for (const downloader of downloaders) {
    if (downloader.type === 'sabnzbd') {
      const client = new SABnzbdClient(downloader.url, downloader.apiKey)
      await client.resume()
    }
  }

  return c.json({ success: true })
})

// DELETE /api/activity/:id - Remove item from queue
activity.delete('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const nzoId = c.req.param('id')
  const downloaderId = c.req.query('downloaderId')

  if (!downloaderId) {
    return c.json({ error: 'downloaderId query parameter required' }, 400)
  }

  const downloader = configService.getDownloader(downloaderId)
  if (!downloader) {
    return c.json({ error: 'Downloader not found' }, 404)
  }

  if (downloader.type === 'sabnzbd') {
    const client = new SABnzbdClient(downloader.url, downloader.apiKey)
    await client.deleteItem(nzoId)
  }

  return c.json({ success: true })
})

export default activity
