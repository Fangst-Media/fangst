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

// TV Series details with status and episode info
const tmdbEpisodeSchema = z.object({
  id: z.number(),
  name: z.string(),
  overview: z.string().optional(),
  episode_number: z.number(),
  season_number: z.number(),
  air_date: z.string().nullable().optional(),
  still_path: z.string().nullable().optional(),
  vote_average: z.number().optional(),
})

const tmdbSeasonSchema = z.object({
  id: z.number(),
  name: z.string(),
  overview: z.string().optional(),
  season_number: z.number(),
  air_date: z.string().nullable().optional(),
  poster_path: z.string().nullable().optional(),
  episode_count: z.number().optional(),
  episodes: z.array(tmdbEpisodeSchema).optional(),
})

const tmdbTvDetailsSchema = z.object({
  id: z.number(),
  name: z.string(),
  overview: z.string().optional(),
  first_air_date: z.string().optional(),
  poster_path: z.string().nullable().optional(),
  backdrop_path: z.string().nullable().optional(),
  vote_average: z.number().optional(),
  status: z.string().optional(),
  number_of_seasons: z.number().optional(),
  number_of_episodes: z.number().optional(),
  seasons: z.array(tmdbSeasonSchema).optional(),
  next_episode_to_air: tmdbEpisodeSchema.nullable().optional(),
  last_episode_to_air: tmdbEpisodeSchema.nullable().optional(),
})

const tmdbSeasonDetailsSchema = z.object({
  id: z.number(),
  name: z.string(),
  overview: z.string().optional(),
  season_number: z.number(),
  air_date: z.string().nullable().optional(),
  poster_path: z.string().nullable().optional(),
  episodes: z.array(tmdbEpisodeSchema),
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

export type TvSeriesStatus = 'returning' | 'ended' | 'upcoming' | 'unknown'

export type TvSeriesDetails = {
  id: string
  title: string
  year?: number
  overview?: string
  poster?: string
  backdrop?: string
  rating?: number
  status: TvSeriesStatus
  numberOfSeasons: number
  numberOfEpisodes: number
  seasons: Array<{
    seasonNumber: number
    name: string
    episodeCount: number
    airDate?: string
  }>
  nextEpisodeToAir?: {
    seasonNumber: number
    episodeNumber: number
    name: string
    airDate?: string
  }
  lastEpisodeToAir?: {
    seasonNumber: number
    episodeNumber: number
    name: string
    airDate?: string
  }
}

export type SeasonEpisode = {
  episodeNumber: number
  seasonNumber: number
  name: string
  overview?: string
  airDate?: string
}

export type SeasonDetails = {
  seasonNumber: number
  name: string
  airDate?: string
  episodes: SeasonEpisode[]
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

  async getTvDetails(tmdbId: string | number): Promise<TvSeriesDetails> {
    const result = await this.fetch(`/tmdb/tv/${tmdbId}`, tmdbTvDetailsSchema)

    return {
      id: result.id.toString(),
      title: result.name,
      year: result.first_air_date ? new Date(result.first_air_date).getFullYear() : undefined,
      overview: result.overview,
      poster: result.poster_path || undefined,
      backdrop: result.backdrop_path || undefined,
      rating: result.vote_average,
      status: this.mapTmdbStatus(result.status),
      numberOfSeasons: result.number_of_seasons ?? 0,
      numberOfEpisodes: result.number_of_episodes ?? 0,
      seasons:
        result.seasons
          ?.filter((s) => s.season_number > 0) // Exclude "specials" season 0
          .map((s) => ({
            seasonNumber: s.season_number,
            name: s.name,
            episodeCount: s.episode_count ?? 0,
            airDate: s.air_date || undefined,
          })) ?? [],
      nextEpisodeToAir: result.next_episode_to_air
        ? {
            seasonNumber: result.next_episode_to_air.season_number,
            episodeNumber: result.next_episode_to_air.episode_number,
            name: result.next_episode_to_air.name,
            airDate: result.next_episode_to_air.air_date || undefined,
          }
        : undefined,
      lastEpisodeToAir: result.last_episode_to_air
        ? {
            seasonNumber: result.last_episode_to_air.season_number,
            episodeNumber: result.last_episode_to_air.episode_number,
            name: result.last_episode_to_air.name,
            airDate: result.last_episode_to_air.air_date || undefined,
          }
        : undefined,
    }
  }

  async getSeasonDetails(tmdbId: string | number, seasonNumber: number): Promise<SeasonDetails> {
    const result = await this.fetch(`/tmdb/tv/${tmdbId}/season/${seasonNumber}`, tmdbSeasonDetailsSchema)

    return {
      seasonNumber: result.season_number,
      name: result.name,
      airDate: result.air_date || undefined,
      episodes: result.episodes.map((ep) => ({
        episodeNumber: ep.episode_number,
        seasonNumber: ep.season_number,
        name: ep.name,
        overview: ep.overview,
        airDate: ep.air_date || undefined,
      })),
    }
  }

  private mapTmdbStatus(status?: string): TvSeriesStatus {
    if (!status) return 'unknown'
    const normalized = status.toLowerCase()
    if (normalized.includes('returning') || normalized.includes('continuing')) return 'returning'
    if (normalized.includes('ended') || normalized.includes('canceled')) return 'ended'
    if (normalized.includes('production') || normalized.includes('planned')) return 'upcoming'
    return 'unknown'
  }
}
