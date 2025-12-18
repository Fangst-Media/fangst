import { z } from 'zod'

const tmdbMovieResultSchema = z.object({
  id: z.number(),
  title: z.string(),
  release_date: z.string().optional(),
  overview: z.string().optional(),
  poster_path: z.string().nullable().optional(),
  backdrop_path: z.string().nullable().optional(),
  vote_average: z.number().optional(),
})

const tmdbTvResultSchema = z.object({
  id: z.number(),
  name: z.string(),
  first_air_date: z.string().optional(),
  overview: z.string().optional(),
  poster_path: z.string().nullable().optional(),
  backdrop_path: z.string().nullable().optional(),
  vote_average: z.number().optional(),
})

const tmdbMovieSearchResponseSchema = z.object({
  results: z.array(tmdbMovieResultSchema),
  page: z.number(),
  total_results: z.number(),
  total_pages: z.number(),
})

const tmdbTvSearchResponseSchema = z.object({
  results: z.array(tmdbTvResultSchema),
  page: z.number(),
  total_results: z.number(),
  total_pages: z.number(),
})

export type MetadataRelayResult = {
  id: string
  title: string
  year?: number
  overview?: string
  poster?: string
  backdrop?: string
  rating?: number
  type: 'movie' | 'tv'
}

export class MetadataRelayClient {
  private baseUrl: string
  private userAgent: string

  constructor(baseUrl = 'https://metadata-relay.dorninger.co', userAgent = 'Jaeger/0.1.0') {
    this.baseUrl = baseUrl
    this.userAgent = userAgent
  }

  private async fetch<T>(endpoint: string, schema: z.ZodSchema<T>): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`
    const response = await fetch(url, {
      headers: {
        'User-Agent': this.userAgent,
        Accept: 'application/json',
      },
    })

    if (!response.ok) {
      throw new Error(`Metadata Relay error: ${response.statusText}`)
    }

    const data = await response.json()
    return schema.parse(data)
  }

  async search(query: string, type?: 'movie' | 'tv'): Promise<MetadataRelayResult[]> {
    if (type === 'movie') {
      return this.searchMovie(query)
    }
    if (type === 'tv') {
      return this.searchTv(query)
    }

    const [movies, tvShows] = await Promise.all([
      this.searchMovie(query).catch(() => []),
      this.searchTv(query).catch(() => []),
    ])
    return [...movies, ...tvShows]
  }

  async searchMovie(query: string, year?: number): Promise<MetadataRelayResult[]> {
    const result = await this.fetch(
      `/tmdb/movies/search?query=${encodeURIComponent(query)}`,
      tmdbMovieSearchResponseSchema
    )

    return result.results
      .filter((movie) => {
        if (!year) return true
        const releaseYear = movie.release_date ? new Date(movie.release_date).getFullYear() : undefined
        return releaseYear === year
      })
      .map((movie) => ({
        id: movie.id.toString(),
        title: movie.title,
        year: movie.release_date ? new Date(movie.release_date).getFullYear() : undefined,
        overview: movie.overview,
        poster: movie.poster_path || undefined,
        backdrop: movie.backdrop_path || undefined,
        rating: movie.vote_average,
        type: 'movie' as const,
      }))
  }

  async searchTv(query: string, year?: number): Promise<MetadataRelayResult[]> {
    const result = await this.fetch(
      `/tmdb/tv/search?query=${encodeURIComponent(query)}`,
      tmdbTvSearchResponseSchema
    )

    return result.results
      .filter((show) => {
        if (!year) return true
        const firstAirYear = show.first_air_date ? new Date(show.first_air_date).getFullYear() : undefined
        return firstAirYear === year
      })
      .map((show) => ({
        id: show.id.toString(),
        title: show.name,
        year: show.first_air_date ? new Date(show.first_air_date).getFullYear() : undefined,
        overview: show.overview,
        poster: show.poster_path || undefined,
        backdrop: show.backdrop_path || undefined,
        rating: show.vote_average,
        type: 'tv' as const,
      }))
  }
}
