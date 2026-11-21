import type {
  BookDetails,
  CollectionDetails,
  LaneResponse,
  MovieDetails,
  TvSeriesDetails,
} from '@fangst/shared/schemas/metadata'
import { Books, Collections, IndexerTrending, Movies, Tv } from '../api'
import { client } from '../api/client.gen'

// Configure the generated client with cookie-based auth
client.setConfig({ credentials: 'include' })

const emptyLane: LaneResponse = { results: [] }

// ---------------------------------------------------------------------------
// Movie lanes
// ---------------------------------------------------------------------------
export const movieLanes = {
  trending: async (): Promise<LaneResponse> => {
    const { data } = await Movies.getApiMetadataTmdbMoviesTrending()
    return (data as LaneResponse) ?? emptyLane
  },
  popular: async (): Promise<LaneResponse> => {
    const { data } = await Movies.getApiMetadataTmdbMoviesPopular()
    return (data as LaneResponse) ?? emptyLane
  },
  upcoming: async (): Promise<LaneResponse> => {
    const { data } = await Movies.getApiMetadataTmdbMoviesUpcoming()
    return (data as LaneResponse) ?? emptyLane
  },
  nowPlaying: async (): Promise<LaneResponse> => {
    const { data } = await Movies.getApiMetadataTmdbMoviesNowPlaying()
    return (data as LaneResponse) ?? emptyLane
  },
  topRated: async (): Promise<LaneResponse> => {
    const { data } = await Movies.getApiMetadataTmdbMoviesTopRated()
    return (data as LaneResponse) ?? emptyLane
  },
  search: async (query: string): Promise<LaneResponse> => {
    const { data } = await Movies.getApiMetadataTmdbMoviesSearch({ query: { query } })
    return (data as LaneResponse) ?? emptyLane
  },
  getById: async (id: string): Promise<MovieDetails> => {
    const { data, error } = await Movies.getApiMetadataTmdbMoviesById({ path: { id } })
    if (error || !data) throw new Error('Failed to fetch movie details')
    return data as MovieDetails
  },
}

// ---------------------------------------------------------------------------
// TV lanes
// ---------------------------------------------------------------------------
export const tvLanes = {
  trending: async (): Promise<LaneResponse> => {
    const { data } = await Tv.getApiMetadataTmdbTvTrending()
    return (data as LaneResponse) ?? emptyLane
  },
  popular: async (): Promise<LaneResponse> => {
    const { data } = await Tv.getApiMetadataTmdbTvPopular()
    return (data as LaneResponse) ?? emptyLane
  },
  topRated: async (): Promise<LaneResponse> => {
    const { data } = await Tv.getApiMetadataTmdbTvTopRated()
    return (data as LaneResponse) ?? emptyLane
  },
  onTheAir: async (): Promise<LaneResponse> => {
    const { data } = await Tv.getApiMetadataTmdbTvOnTheAir()
    return (data as LaneResponse) ?? emptyLane
  },
  search: async (query: string): Promise<LaneResponse> => {
    const { data } = await Tv.getApiMetadataTmdbTvSearch({ query: { query } })
    return (data as LaneResponse) ?? emptyLane
  },
  getById: async (id: string): Promise<TvSeriesDetails> => {
    const { data, error } = await Tv.getApiMetadataTmdbTvById({ path: { id } })
    if (error || !data) throw new Error('Failed to fetch TV details')
    return data as TvSeriesDetails
  },
}

// ---------------------------------------------------------------------------
// Book lanes
// ---------------------------------------------------------------------------
export const bookLanes = {
  trending: async (): Promise<LaneResponse> => {
    const { data } = await Books.getApiMetadataBooksTrending()
    return (data as LaneResponse) ?? emptyLane
  },
  popular: async (): Promise<LaneResponse> => {
    const { data } = await Books.getApiMetadataBooksPopular()
    return (data as LaneResponse) ?? emptyLane
  },
  classics: async (): Promise<LaneResponse> => {
    const { data } = await Books.getApiMetadataBooksClassics()
    return (data as LaneResponse) ?? emptyLane
  },
  search: async (query: string): Promise<LaneResponse> => {
    const { data } = await Books.getApiMetadataBooksSearch({ query: { query } })
    return (data as LaneResponse) ?? emptyLane
  },
  getByWorkId: async (workId: string): Promise<BookDetails> => {
    const { data, error } = await Books.getApiMetadataBooksByWorkId({ path: { workId } })
    if (error || !data) throw new Error('Failed to fetch book details')
    return data as BookDetails
  },
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------
export const collections = {
  getById: async (id: string): Promise<CollectionDetails> => {
    const { data, error } = await Collections.getApiMetadataTmdbCollectionsById({ path: { id } })
    if (error || !data) throw new Error('Failed to fetch collection details')
    return data as CollectionDetails
  },
}

// ---------------------------------------------------------------------------
// Indexer trending
// ---------------------------------------------------------------------------
export const indexerTrending = {
  movies: async (): Promise<LaneResponse> => {
    const { data } = await IndexerTrending.getApiMetadataIndexerTrendingMovies()
    return (data as LaneResponse) ?? emptyLane
  },
  tv: async (): Promise<LaneResponse> => {
    const { data } = await IndexerTrending.getApiMetadataIndexerTrendingTv()
    return (data as LaneResponse) ?? emptyLane
  },
}

// ---------------------------------------------------------------------------
// Typed fetch helper for non-OpenAPI routes
// ---------------------------------------------------------------------------
export async function apiFetch<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await fetch(path, { credentials: 'include', ...options })
  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: response.statusText }))
    throw new Error(body.error || `Request failed: ${response.status}`)
  }
  return response.json()
}

export async function apiPost<T>(path: string, body: unknown): Promise<T> {
  return apiFetch<T>(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
}
