import { z } from 'zod'

const tvdbSearchResultSchema = z.object({
  objectID: z.string(),
  name: z.string(),
  year: z.string().optional(),
  overview: z.string().optional(),
  image_url: z.string().optional(),
  type: z.string(),
})

const tvdbSearchResponseSchema = z.object({
  data: z.array(tvdbSearchResultSchema),
})

const tvdbMovieSchema = z.object({
  id: z.number(),
  name: z.string(),
  year: z.string().optional(),
  overview: z.string().optional(),
  image: z.string().optional(),
})

const tvdbSeriesSchema = z.object({
  id: z.number(),
  name: z.string(),
  year: z.string().optional(),
  overview: z.string().optional(),
  image: z.string().optional(),
})

export type TvdbSearchResult = z.infer<typeof tvdbSearchResultSchema>
export type TvdbMovie = z.infer<typeof tvdbMovieSchema>
export type TvdbSeries = z.infer<typeof tvdbSeriesSchema>

export class TvdbClient {
  private baseUrl = 'https://api4.thetvdb.com/v4'
  private apiKey: string
  private token?: string

  constructor(apiKey: string) {
    this.apiKey = apiKey
  }

  private async ensureToken(): Promise<void> {
    if (this.token) return

    const response = await fetch(`${this.baseUrl}/login`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ apikey: this.apiKey }),
    })

    if (!response.ok) {
      throw new Error(`TVDB authentication failed: ${response.statusText}`)
    }

    const data = (await response.json()) as { data: { token: string } }
    this.token = data.data.token
  }

  private async fetch<T>(endpoint: string, schema: z.ZodSchema<T>): Promise<T> {
    await this.ensureToken()

    const response = await fetch(`${this.baseUrl}${endpoint}`, {
      headers: {
        Authorization: `Bearer ${this.token}`,
        Accept: 'application/json',
      },
    })

    if (!response.ok) {
      throw new Error(`TVDB API error: ${response.statusText}`)
    }

    const data = await response.json()
    return schema.parse(data)
  }

  async search(query: string): Promise<TvdbSearchResult[]> {
    const result = await this.fetch(
      `/search?query=${encodeURIComponent(query)}`,
      tvdbSearchResponseSchema
    )
    return result.data
  }

  async getMovie(id: number): Promise<TvdbMovie> {
    const result = await this.fetch(`/movies/${id}`, z.object({ data: tvdbMovieSchema }))
    return result.data
  }

  async getSeries(id: number): Promise<TvdbSeries> {
    const result = await this.fetch(`/series/${id}`, z.object({ data: tvdbSeriesSchema }))
    return result.data
  }
}
