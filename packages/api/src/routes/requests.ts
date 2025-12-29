import { MetadataRelayClient } from '@jaeger/shared/clients/metadata-relay'
import { Hono } from 'hono'
import { runAirDateMonitor } from '../jobs/airDateMonitor'
import { runDownloadMonitor } from '../jobs/downloadMonitor'
import { runEpisodeSearchJob } from '../jobs/episodeSearch'
import { runSearchJob } from '../jobs/searchRequests'
import type { User } from '../services/auth'
import { configService } from '../services/config'
import { episodeService, type MonitoringPreset } from '../services/episodes'
import { type CreateRequestData, type RequestStatus, requestService } from '../services/requests'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const requestsRouter = new Hono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

function requireAdmin(user: User | null): user is User {
  return user !== null && user.role === 'admin'
}

// GET /api/requests - List all requests
requestsRouter.get('/', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const status = c.req.query('status') as RequestStatus | undefined
  const type = c.req.query('type') as 'movie' | 'tv' | undefined
  const mine = c.req.query('mine') === 'true'

  const filters: {
    status?: RequestStatus | RequestStatus[]
    userId?: string
    type?: 'movie' | 'tv'
  } = {}

  if (status) {
    filters.status = status
  }

  if (type) {
    filters.type = type
  }

  // Non-admins can only see their own requests by default
  if (mine || user.role !== 'admin') {
    filters.userId = user.id
  }

  const items = await requestService.getAll(filters)

  return c.json({ requests: items })
})

// POST /api/requests - Create new request
requestsRouter.post('/', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const body = (await c.req.json()) as CreateRequestData & { downloading?: boolean }

    if (!body.title || !body.type) {
      return c.json({ error: 'Missing required fields: title, type' }, 400)
    }

    const isAdmin = user.role === 'admin'
    const request = await requestService.create(body, user.id, isAdmin)

    // If this request is being created alongside a download, mark it as downloading
    if (body.downloading) {
      await requestService.updateStatus(request.id, 'downloading')
      const updated = await requestService.getById(request.id)
      return c.json({ success: true, request: updated })
    }

    return c.json({ success: true, request })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to create request' },
      400
    )
  }
})

// GET /api/requests/season/:tmdbId/:seasonNumber - Find request for a specific season
requestsRouter.get('/season/:tmdbId/:seasonNumber', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const tmdbId = c.req.param('tmdbId')
  const seasonNumber = parseInt(c.req.param('seasonNumber'), 10)

  if (isNaN(seasonNumber)) {
    return c.json({ error: 'Invalid season number' }, 400)
  }

  const request = await requestService.getByTmdbIdAndSeason(tmdbId, seasonNumber)

  if (!request) {
    return c.json({ request: null })
  }

  // Include episodes if request exists
  const episodes = await requestService.getEpisodes(request.id)

  return c.json({ request, episodes })
})

// GET /api/requests/:id - Get specific request
requestsRouter.get('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const id = c.req.param('id')
  const request = await requestService.getById(id)

  if (!request) {
    return c.json({ error: 'Request not found' }, 404)
  }

  // Non-admins can only see their own requests
  if (user.role !== 'admin' && request.requestedBy.id !== user.id) {
    return c.json({ error: 'Not authorized' }, 403)
  }

  return c.json({ request })
})

// DELETE /api/requests/:id - Delete request
requestsRouter.delete('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const id = c.req.param('id')
    const isAdmin = user.role === 'admin'
    await requestService.delete(id, user.id, isAdmin)

    return c.json({ success: true })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to delete request' },
      400
    )
  }
})

// POST /api/requests/:id/approve - Approve pending request (admin only)
requestsRouter.post('/:id/approve', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    const request = await requestService.approve(id, user.id)

    return c.json({ success: true, request })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to approve request' },
      400
    )
  }
})

// POST /api/requests/:id/reject - Reject pending request (admin only)
requestsRouter.post('/:id/reject', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    const request = await requestService.reject(id, user.id)

    return c.json({ success: true, request })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to reject request' },
      400
    )
  }
})

// POST /api/requests/:id/retry - Retry failed request (admin only)
requestsRouter.post('/:id/retry', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    await requestService.updateStatus(id, 'approved')
    const request = await requestService.getById(id)

    return c.json({ success: true, request })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to retry request' },
      400
    )
  }
})

// POST /api/requests/search-job - Manually trigger search job (admin only)
requestsRouter.post('/search-job', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const result = await runSearchJob()
    return c.json({ success: true, ...result })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to run search job' },
      500
    )
  }
})

// POST /api/requests/monitor-job - Check download status (admin only)
requestsRouter.post('/monitor-job', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const result = await runDownloadMonitor()
    return c.json({ success: true, ...result })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to run download monitor' },
      500
    )
  }
})

// POST /api/requests/episode-search-job - Search for TV episodes (admin only)
requestsRouter.post('/episode-search-job', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const result = await runEpisodeSearchJob()
    return c.json({ success: true, ...result })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to run episode search job' },
      500
    )
  }
})

// POST /api/requests/air-date-monitor - Check for new episodes (admin only)
requestsRouter.post('/air-date-monitor', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const result = await runAirDateMonitor()
    return c.json({ success: true, ...result })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to run air date monitor' },
      500
    )
  }
})

// GET /api/requests/:id/episodes - Get episodes for a TV request
requestsRouter.get('/:id/episodes', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const id = c.req.param('id')
  const request = await requestService.getById(id)

  if (!request) {
    return c.json({ error: 'Request not found' }, 404)
  }

  // Non-admins can only see their own requests
  if (user.role !== 'admin' && !request.requestedByUsers.some((u) => u.id === user.id)) {
    return c.json({ error: 'Not authorized' }, 403)
  }

  const episodes = await requestService.getEpisodes(id)

  return c.json({ episodes })
})

// PATCH /api/requests/:id/episodes/:episodeId - Toggle episode monitored status
requestsRouter.patch('/:id/episodes/:episodeId', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const requestId = c.req.param('id')
  const episodeId = c.req.param('episodeId')
  const request = await requestService.getById(requestId)

  if (!request) {
    return c.json({ error: 'Request not found' }, 404)
  }

  if (user.role !== 'admin' && !request.requestedByUsers.some((u) => u.id === user.id)) {
    return c.json({ error: 'Not authorized' }, 403)
  }

  try {
    const body = (await c.req.json()) as { monitored: boolean }

    if (typeof body.monitored !== 'boolean') {
      return c.json({ error: 'monitored must be a boolean' }, 400)
    }

    await episodeService.setMonitored(episodeId, body.monitored)
    const episode = await episodeService.getById(episodeId)

    return c.json({ success: true, episode })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to update episode' },
      500
    )
  }
})

// POST /api/requests/:id/episodes/fetch - Fetch episodes from TMDB and create records
requestsRouter.post('/:id/episodes/fetch', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const id = c.req.param('id')
  const request = await requestService.getById(id)

  if (!request) {
    return c.json({ error: 'Request not found' }, 404)
  }

  // Allow admins or the user who created the request
  if (user.role !== 'admin' && !request.requestedByUsers.some((u) => u.id === user.id)) {
    return c.json({ error: 'Not authorized' }, 403)
  }

  if (request.type !== 'tv' || !request.season) {
    return c.json({ error: 'Not a TV season request' }, 400)
  }

  if (!request.tmdbId) {
    return c.json({ error: 'No TMDB ID for this request' }, 400)
  }

  try {
    const relayUrl = configService.getMetadataRelayUrl()
    const client = new MetadataRelayClient(relayUrl)

    // Fetch series details to get status
    const seriesDetails = await client.getTvDetails(request.tmdbId)

    // Update series status
    await requestService.updateRequestSeriesStatus(request.id, seriesDetails.status)

    // Fetch season details
    const seasonDetails = await client.getSeasonDetails(request.tmdbId, request.season)

    // Check existing episodes
    const existingEpisodes = await requestService.getEpisodes(request.id)
    const existingEpNumbers = new Set(existingEpisodes.map((ep) => ep.episodeNumber))

    // Add new episodes
    const newEpisodes = seasonDetails.episodes.filter(
      (ep) => !existingEpNumbers.has(ep.episodeNumber)
    )

    if (newEpisodes.length > 0) {
      const preset = (request.monitoringPreset as MonitoringPreset) ?? 'all'
      const created = await requestService.createEpisodesForSeason(
        request.id,
        newEpisodes.map((ep) => ({
          seasonNumber: ep.seasonNumber,
          episodeNumber: ep.episodeNumber,
          title: ep.name,
          airDate: ep.airDate,
        })),
        preset
      )

      return c.json({
        success: true,
        seriesStatus: seriesDetails.status,
        episodesAdded: created.length,
        totalEpisodes: existingEpisodes.length + created.length,
      })
    }

    return c.json({
      success: true,
      seriesStatus: seriesDetails.status,
      episodesAdded: 0,
      totalEpisodes: existingEpisodes.length,
    })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch episodes' },
      500
    )
  }
})

export default requestsRouter
