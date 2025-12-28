import { Hono } from 'hono'
import type { User } from '../services/auth'
import { configService } from '../services/config'
import { getMetadataService, type MetadataResult } from '../services/metadata'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const metadata = new Hono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

function getService() {
  return getMetadataService({
    relayUrl: configService.getMetadataRelayUrl(),
    tmdbApiKey: configService.getTmdbApiKey() || undefined,
  })
}

type LaneResponse = {
  results: MetadataResult[]
  unsupported?: boolean
}

// Trending movies
metadata.get('/tmdb/movies/trending', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const service = getService()
    const results = await service.getTrendingMovies()

    if (results === null) {
      return c.json({ results: [], unsupported: true } satisfies LaneResponse)
    }

    return c.json({ results } satisfies LaneResponse)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch trending movies' },
      500
    )
  }
})

// Trending TV shows - MUST be before /tmdb/tv/:id
metadata.get('/tmdb/tv/trending', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const service = getService()
    const results = await service.getTrendingTv()

    if (results === null) {
      return c.json({ results: [], unsupported: true } satisfies LaneResponse)
    }

    return c.json({ results } satisfies LaneResponse)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch trending TV' },
      500
    )
  }
})

// Popular movies
metadata.get('/tmdb/movies/popular', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const service = getService()
    const results = await service.getPopularMovies()

    if (results === null) {
      return c.json({ results: [], unsupported: true } satisfies LaneResponse)
    }

    return c.json({ results } satisfies LaneResponse)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch popular movies' },
      500
    )
  }
})

// Upcoming movies
metadata.get('/tmdb/movies/upcoming', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const service = getService()
    const results = await service.getUpcomingMovies()

    if (results === null) {
      return c.json({ results: [], unsupported: true } satisfies LaneResponse)
    }

    return c.json({ results } satisfies LaneResponse)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch upcoming movies' },
      500
    )
  }
})

// Now playing movies
metadata.get('/tmdb/movies/now_playing', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const service = getService()
    const results = await service.getNowPlayingMovies()

    if (results === null) {
      return c.json({ results: [], unsupported: true } satisfies LaneResponse)
    }

    return c.json({ results } satisfies LaneResponse)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch now playing movies' },
      500
    )
  }
})

// Top rated movies
metadata.get('/tmdb/movies/top_rated', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const service = getService()
    const results = await service.getTopRatedMovies()

    if (results === null) {
      return c.json({ results: [], unsupported: true } satisfies LaneResponse)
    }

    return c.json({ results } satisfies LaneResponse)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch top rated movies' },
      500
    )
  }
})

// Popular TV shows - MUST be before /tmdb/tv/:id
metadata.get('/tmdb/tv/popular', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const service = getService()
    const results = await service.getPopularTv()

    if (results === null) {
      return c.json({ results: [], unsupported: true } satisfies LaneResponse)
    }

    return c.json({ results } satisfies LaneResponse)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch popular TV' },
      500
    )
  }
})

// Top rated TV shows - MUST be before /tmdb/tv/:id
metadata.get('/tmdb/tv/top_rated', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const service = getService()
    const results = await service.getTopRatedTv()

    if (results === null) {
      return c.json({ results: [], unsupported: true } satisfies LaneResponse)
    }

    return c.json({ results } satisfies LaneResponse)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch top rated TV' },
      500
    )
  }
})

// On air TV shows - MUST be before /tmdb/tv/:id
metadata.get('/tmdb/tv/on_the_air', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const service = getService()
    const results = await service.getOnAirTv()

    if (results === null) {
      return c.json({ results: [], unsupported: true } satisfies LaneResponse)
    }

    return c.json({ results } satisfies LaneResponse)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch on air TV' },
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
    const service = getService()
    const details = await service.getTvDetails(tmdbId)

    if (details === null) {
      return c.json({ error: 'No provider supports TV details' }, 501)
    }

    return c.json(details)
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch TV details' },
      500
    )
  }
})

// Proxy movie and TV search requests
metadata.get('/tmdb/movies/search', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const query = c.req.query('query')
  if (!query) {
    return c.json({ error: 'Query parameter required' }, 400)
  }

  try {
    const service = getService()
    const results = await service.searchMovie(query)

    if (results === null) {
      return c.json({ results: [], unsupported: true })
    }

    return c.json({ results })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to search movies' },
      500
    )
  }
})

metadata.get('/tmdb/tv/search', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const query = c.req.query('query')
  if (!query) {
    return c.json({ error: 'Query parameter required' }, 400)
  }

  try {
    const service = getService()
    const results = await service.searchTv(query)

    if (results === null) {
      return c.json({ results: [], unsupported: true })
    }

    return c.json({ results })
  } catch (error) {
    return c.json({ error: error instanceof Error ? error.message : 'Failed to search TV' }, 500)
  }
})

export default metadata
