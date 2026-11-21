import { OpenAPIHono } from '@hono/zod-openapi'
import type { User } from '../services/auth'
import { collectionService } from '../services/collections'
import { libraryService } from '../services/library'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const libraryRouter = new OpenAPIHono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

// GET /api/library/collections - List all collections in library
libraryRouter.get('/collections', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const collections = await collectionService.getCollectionsInLibrary()
    return c.json({ collections })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch collections' },
      500
    )
  }
})

// GET /api/library/collections/:tmdbCollectionId - Get collection with movies
libraryRouter.get('/collections/:tmdbCollectionId', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const tmdbCollectionId = Number.parseInt(c.req.param('tmdbCollectionId'), 10)
  if (isNaN(tmdbCollectionId)) {
    return c.json({ error: 'Invalid collection ID' }, 400)
  }

  try {
    const data = await collectionService.getCollectionWithMovies(tmdbCollectionId)

    if (!data) {
      return c.json({ error: 'Collection not found' }, 404)
    }

    return c.json(data)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch collection' },
      500
    )
  }
})

// GET /api/library/tv - List all TV series in library
libraryRouter.get('/tv', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const series = await libraryService.getTvSeries()
    return c.json({ series })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch TV series' },
      500
    )
  }
})

// GET /api/library/tv/:tmdbId - Get TV series with season progress
libraryRouter.get('/tv/:tmdbId', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const tmdbId = c.req.param('tmdbId')

  try {
    const data = await libraryService.getTvSeriesById(tmdbId)

    if (!data) {
      return c.json({ error: 'Series not found in library' }, 404)
    }

    return c.json(data)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch series details' },
      500
    )
  }
})

// GET /api/library/tv/:tmdbId/season/:seasonNumber - Get season episodes with download status
libraryRouter.get('/tv/:tmdbId/season/:seasonNumber', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const tmdbId = c.req.param('tmdbId')
  const seasonNumber = Number.parseInt(c.req.param('seasonNumber'), 10)

  if (isNaN(seasonNumber)) {
    return c.json({ error: 'Invalid season number' }, 400)
  }

  try {
    const data = await libraryService.getSeasonEpisodes(tmdbId, seasonNumber)

    if (!data) {
      return c.json({ error: 'Season not found' }, 404)
    }

    return c.json(data)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch season details' },
      500
    )
  }
})

export default libraryRouter
