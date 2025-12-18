import { z } from 'zod'

const tmdbMovieSchema = z.object({
  id: z.number(),
  title: z.string(),
  original_title: z.string(),
  overview: z.string().nullable(),
  release_date: z.string(),
  poster_path: z.string().nullable(),
  backdrop_path: z.string().nullable(),
  vote_average: z.number(),
  vote_count: z.number(),
  genres: z.array(z.object({ id: z.number(), name: z.string() })).optional(),
})

const tmdbTvSchema = z.object({
  id: z.number(),
  name: z.string(),
  original_name: z.string(),
  overview: z.string().nullable(),
  first_air_date: z.string(),
  poster_path: z.string().nullable(),
  backdrop_path: z.string().nullable(),
  vote_average: z.number(),
  vote_count: z.number(),
  genres: z.array(z.object({ id: z.number(), name: z.string() })).optional(),
})

const tmdbSearchResultSchema = z.object({
  page: z.number(),
  results: z.array(
    z.union([
      tmdbMovieSchema.extend({ media_type: z.literal('movie') }),
      tmdbTvSchema.extend({ media_type: z.literal('tv') }),
    ])
  ),
  total_pages: z.number(),
  total_results: z.number(),
})

export type TmdbMovie = z.infer<typeof tmdbMovieSchema>
export type TmdbTv = z.infer<typeof tmdbTvSchema>
export type TmdbSearchResult = z.infer<typeof tmdbSearchResultSchema>

export class TmdbClient {
  private baseUrl = 'https://api.themoviedb.org/3'
  private apiKey: string

  constructor(apiKey: string) {
    this.apiKey = apiKey
  }

  private async fetch<T>(endpoint: string, schema: z.ZodSchema<T>): Promise<T> {
    const url = `${this.baseUrl}${endpoint}${endpoint.includes('?') ? '&' : '?'}api_key=${this.apiKey}`
    const response = await fetch(url)

    if (!response.ok) {
      throw new Error(`TMDB API error: ${response.statusText}`)
    }

    const data = await response.json()
    return schema.parse(data)
  }

  async searchMulti(query: string, year?: number): Promise<TmdbSearchResult> {
    const yearParam = year ? `&year=${year}` : ''
    return this.fetch(`/search/multi?query=${encodeURIComponent(query)}${yearParam}`, tmdbSearchResultSchema)
  }

  async getMovie(id: number): Promise<TmdbMovie> {
    return this.fetch(`/movie/${id}`, tmdbMovieSchema)
  }

  async getTv(id: number): Promise<TmdbTv> {
    return this.fetch(`/tv/${id}`, tmdbTvSchema)
  }

  async searchMovie(query: string, year?: number): Promise<TmdbSearchResult> {
    const yearParam = year ? `&year=${year}` : ''
    return this.fetch(
      `/search/movie?query=${encodeURIComponent(query)}${yearParam}`,
      tmdbSearchResultSchema
    )
  }

  async searchTv(query: string, year?: number): Promise<TmdbSearchResult> {
    const yearParam = year ? `&first_air_date_year=${year}` : ''
    return this.fetch(
      `/search/tv?query=${encodeURIComponent(query)}${yearParam}`,
      tmdbSearchResultSchema
    )
  }
}
