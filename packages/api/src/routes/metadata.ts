import { OpenAPIHono } from '@hono/zod-openapi'
import type { User } from '../services/auth'
import { configService } from '../services/config'
import { indexerTrendingService } from '../services/indexer-trending'
import { getMetadataService } from '../services/metadata'
import { MetadataStore } from '../services/metadata-store'
import {
  movieDetailsSchema,
  tvSeriesDetailsSchema,
  collectionDetailsSchema,
} from '@jaeger/shared/schemas/metadata'
import { createLaneRoute, createSearchRoute, createDetailRoute } from '../lib/openapi-helpers'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const metadata = new OpenAPIHono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

function getStore() {
  const relayUrl = configService.getMetadataRelayUrl()
  const tmdbApiKey = configService.getTmdbApiKey()
  const service = getMetadataService({ relayUrl, tmdbApiKey: tmdbApiKey || undefined })
  return new MetadataStore(service)
}

// Lane handler factory — all lane endpoints share the same logic
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

// --- Movie lanes ---

metadata.openapi(
  createLaneRoute('/tmdb/movies/trending', 'Get trending movies', 'Movies'),
  laneHandler((s) => s.getTrendingMovies())
)

metadata.openapi(
  createLaneRoute('/tmdb/movies/popular', 'Get popular movies', 'Movies'),
  laneHandler((s) => s.getPopularMovies())
)

metadata.openapi(
  createLaneRoute('/tmdb/movies/upcoming', 'Get upcoming movies', 'Movies'),
  laneHandler((s) => s.getUpcomingMovies())
)

metadata.openapi(
  createLaneRoute('/tmdb/movies/now_playing', 'Get now playing movies', 'Movies'),
  laneHandler((s) => s.getNowPlayingMovies())
)

metadata.openapi(
  createLaneRoute('/tmdb/movies/top_rated', 'Get top rated movies', 'Movies'),
  laneHandler((s) => s.getTopRatedMovies())
)

// --- TV lanes ---

metadata.openapi(
  createLaneRoute('/tmdb/tv/trending', 'Get trending TV shows', 'TV'),
  laneHandler((s) => s.getTrendingTv())
)

metadata.openapi(
  createLaneRoute('/tmdb/tv/popular', 'Get popular TV shows', 'TV'),
  laneHandler((s) => s.getPopularTv())
)

metadata.openapi(
  createLaneRoute('/tmdb/tv/top_rated', 'Get top rated TV shows', 'TV'),
  laneHandler((s) => s.getTopRatedTv())
)

metadata.openapi(
  createLaneRoute('/tmdb/tv/on_the_air', 'Get on air TV shows', 'TV'),
  laneHandler((s) => s.getOnAirTv())
)

// --- Search (must be before :id routes) ---

function searchHandler(searchFn: (store: MetadataStore, query: string) => Promise<unknown[] | null>) {
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

metadata.openapi(
  createSearchRoute('/tmdb/movies/search', 'Search movies', 'Movies'),
  searchHandler((s, q) => s.searchMovie(q))
)

metadata.openapi(
  createSearchRoute('/tmdb/tv/search', 'Search TV shows', 'TV'),
  searchHandler((s, q) => s.searchTv(q))
)

// --- Detail endpoints ---

function detailHandler(fetchFn: (store: MetadataStore, id: string) => Promise<unknown | null>) {
  return async (c: any) => {
    const user = c.get('user')
    if (!requireAuth(user)) return c.json({ error: 'Authentication required' }, 401)
    const id = c.req.valid('param').id ?? c.req.valid('param').workId ?? ''
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

metadata.openapi(
  createDetailRoute('/tmdb/collections/{id}', collectionDetailsSchema, 'Get collection details', 'Collections'),
  detailHandler((s, id) => s.getCollectionDetails(parseInt(id, 10)))
)

metadata.openapi(
  createDetailRoute('/tmdb/movies/{id}', movieDetailsSchema, 'Get movie details', 'Movies'),
  detailHandler((s, id) => s.getMovieDetails(id))
)

metadata.openapi(
  createDetailRoute('/tmdb/tv/{id}', tvSeriesDetailsSchema, 'Get TV series details', 'TV'),
  detailHandler((s, id) => s.getTvDetails(id))
)

// --- Indexer trending ---

function indexerTrendingHandler(mediaType: 'movie' | 'tv') {
  return async (c: any) => {
    const user = c.get('user')
    if (!requireAuth(user)) return c.json({ error: 'Authentication required' }, 401)
    try {
      const indexerId = c.req.query('indexerId')
      const result = await indexerTrendingService.getTrending(mediaType, indexerId)
      return c.json(result)
    } catch (error) {
      return c.json({ error: error instanceof Error ? error.message : 'Failed' }, 500)
    }
  }
}

metadata.openapi(
  createLaneRoute('/indexer/trending/movies', 'Get trending movies from indexers', 'Indexer Trending'),
  indexerTrendingHandler('movie')
)

metadata.openapi(
  createLaneRoute('/indexer/trending/tv', 'Get trending TV from indexers', 'Indexer Trending'),
  indexerTrendingHandler('tv')
)

export default metadata
