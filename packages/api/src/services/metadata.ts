import { MetadataRelayClient } from '@jaeger/shared/clients/metadata-relay'
import type {
  MetadataRelayResult,
  TvSeriesDetails,
  SeasonDetails,
} from '@jaeger/shared/clients/metadata-relay'

export type { MetadataRelayResult as MetadataResult, TvSeriesDetails, SeasonDetails }

export type MetadataCapability =
  | 'search'
  | 'trending'
  | 'tvDetails'
  | 'movieDetails'
  | 'seasonDetails'
  | 'collectionDetails'
  | 'popular'
  | 'upcoming'
  | 'nowPlaying'
  | 'topRated'
  | 'onAir'

export type MovieDetails = {
  id: string
  title: string
  year?: number
  overview?: string
  poster?: string
  backdrop?: string
  rating?: number
  runtime?: number
  imdbId?: string
  collection?: {
    id: number
    name: string
    posterPath?: string
    backdropPath?: string
  }
}

export type CollectionDetails = {
  id: number
  name: string
  overview?: string
  posterPath?: string
  backdropPath?: string
  movies: {
    id: number
    title: string
    overview?: string
    releaseDate?: string
    posterPath?: string
  }[]
}

export interface MetadataProvider {
  name: string
  capabilities: MetadataCapability[]

  searchMovie?(query: string, year?: number): Promise<MetadataRelayResult[]>
  searchTv?(query: string, year?: number): Promise<MetadataRelayResult[]>
  getTrendingMovies?(): Promise<MetadataRelayResult[]>
  getTrendingTv?(): Promise<MetadataRelayResult[]>
  getPopularMovies?(): Promise<MetadataRelayResult[]>
  getPopularTv?(): Promise<MetadataRelayResult[]>
  getUpcomingMovies?(): Promise<MetadataRelayResult[]>
  getNowPlayingMovies?(): Promise<MetadataRelayResult[]>
  getTopRatedMovies?(): Promise<MetadataRelayResult[]>
  getTopRatedTv?(): Promise<MetadataRelayResult[]>
  getOnAirTv?(): Promise<MetadataRelayResult[]>
  getTvDetails?(tmdbId: string | number): Promise<TvSeriesDetails>
  getMovieDetails?(tmdbId: string | number): Promise<MovieDetails>
  getSeasonDetails?(tmdbId: string | number, seasonNumber: number): Promise<SeasonDetails>
  getCollectionDetails?(collectionId: number): Promise<CollectionDetails>
}

// Wraps the existing MetadataRelayClient
class RelayProvider implements MetadataProvider {
  name = 'relay'
  capabilities: MetadataCapability[] = ['search', 'trending', 'tvDetails', 'seasonDetails']

  private client: MetadataRelayClient

  constructor(relayUrl: string) {
    this.client = new MetadataRelayClient(relayUrl)
  }

  async searchMovie(query: string, year?: number): Promise<MetadataRelayResult[]> {
    return this.client.searchMovie(query, year)
  }

  async searchTv(query: string, year?: number): Promise<MetadataRelayResult[]> {
    return this.client.searchTv(query, year)
  }

  async getTrendingMovies(): Promise<MetadataRelayResult[]> {
    return this.client.getTrendingMovies()
  }

  async getTrendingTv(): Promise<MetadataRelayResult[]> {
    return this.client.getTrendingTv()
  }

  async getTvDetails(tmdbId: string | number): Promise<TvSeriesDetails> {
    return this.client.getTvDetails(tmdbId)
  }

  async getSeasonDetails(tmdbId: string | number, seasonNumber: number): Promise<SeasonDetails> {
    return this.client.getSeasonDetails(tmdbId, seasonNumber)
  }
}

// Direct TMDB API provider with 24-hour caching
class TmdbProvider implements MetadataProvider {
  name = 'tmdb'
  capabilities: MetadataCapability[] = [
    'search',
    'trending',
    'tvDetails',
    'movieDetails',
    'seasonDetails',
    'collectionDetails',
    'popular',
    'upcoming',
    'nowPlaying',
    'topRated',
    'onAir',
  ]

  private apiKey: string
  private baseUrl = 'https://api.themoviedb.org/3'
  private cache = new Map<string, { data: unknown; expires: number }>()
  private cacheTtl = 24 * 60 * 60 * 1000 // 24 hours

  constructor(apiKey: string) {
    this.apiKey = apiKey
  }

  private getCached<T>(key: string): T | null {
    const entry = this.cache.get(key)
    if (!entry) return null
    if (Date.now() > entry.expires) {
      this.cache.delete(key)
      return null
    }
    return entry.data as T
  }

  private setCache(key: string, data: unknown): void {
    this.cache.set(key, { data, expires: Date.now() + this.cacheTtl })
  }

  private async fetch<T>(endpoint: string, cacheKey?: string): Promise<T> {
    const key = cacheKey || endpoint
    const cached = this.getCached<T>(key)
    if (cached) return cached

    const url = `${this.baseUrl}${endpoint}${endpoint.includes('?') ? '&' : '?'}api_key=${this.apiKey}`
    const response = await fetch(url, {
      headers: {
        Accept: 'application/json',
      },
    })

    if (!response.ok) {
      throw new Error(`TMDB API error: ${response.statusText}`)
    }

    const data = await response.json()
    this.setCache(key, data)
    return data as T
  }

  private mapMovie(movie: TmdbMovie): MetadataRelayResult {
    return {
      id: movie.id.toString(),
      title: movie.title,
      year: movie.release_date ? new Date(movie.release_date).getFullYear() : undefined,
      releaseDate: movie.release_date,
      overview: movie.overview,
      poster: movie.poster_path || undefined,
      backdrop: movie.backdrop_path || undefined,
      rating: movie.vote_average,
      type: 'movie',
    }
  }

  private mapTv(show: TmdbTv): MetadataRelayResult {
    return {
      id: show.id.toString(),
      title: show.name,
      year: show.first_air_date ? new Date(show.first_air_date).getFullYear() : undefined,
      overview: show.overview,
      poster: show.poster_path || undefined,
      backdrop: show.backdrop_path || undefined,
      rating: show.vote_average,
      type: 'tv',
    }
  }

  async searchMovie(query: string, year?: number): Promise<MetadataRelayResult[]> {
    const yearParam = year ? `&year=${year}` : ''
    const data = await this.fetch<TmdbMovieResponse>(
      `/search/movie?query=${encodeURIComponent(query)}${yearParam}`,
      `search-movie-${query}-${year}`
    )
    return data.results.map((m) => this.mapMovie(m))
  }

  async searchTv(query: string, year?: number): Promise<MetadataRelayResult[]> {
    const yearParam = year ? `&first_air_date_year=${year}` : ''
    const data = await this.fetch<TmdbTvResponse>(
      `/search/tv?query=${encodeURIComponent(query)}${yearParam}`,
      `search-tv-${query}-${year}`
    )
    return data.results.map((t) => this.mapTv(t))
  }

  async getTrendingMovies(): Promise<MetadataRelayResult[]> {
    const data = await this.fetch<TmdbMovieResponse>('/trending/movie/week', 'trending-movies')
    return data.results.map((m) => this.mapMovie(m))
  }

  async getTrendingTv(): Promise<MetadataRelayResult[]> {
    const data = await this.fetch<TmdbTvResponse>('/trending/tv/week', 'trending-tv')
    return data.results.map((t) => this.mapTv(t))
  }

  async getPopularMovies(): Promise<MetadataRelayResult[]> {
    const data = await this.fetch<TmdbMovieResponse>('/movie/popular', 'popular-movies')
    return data.results.map((m) => this.mapMovie(m))
  }

  async getPopularTv(): Promise<MetadataRelayResult[]> {
    const data = await this.fetch<TmdbTvResponse>('/tv/popular', 'popular-tv')
    return data.results.map((t) => this.mapTv(t))
  }

  async getUpcomingMovies(): Promise<MetadataRelayResult[]> {
    const data = await this.fetch<TmdbMovieResponse>('/movie/upcoming', 'upcoming-movies')
    return data.results.map((m) => this.mapMovie(m))
  }

  async getNowPlayingMovies(): Promise<MetadataRelayResult[]> {
    const data = await this.fetch<TmdbMovieResponse>('/movie/now_playing', 'now-playing-movies')
    return data.results.map((m) => this.mapMovie(m))
  }

  async getTopRatedMovies(): Promise<MetadataRelayResult[]> {
    const data = await this.fetch<TmdbMovieResponse>('/movie/top_rated', 'top-rated-movies')
    return data.results.map((m) => this.mapMovie(m))
  }

  async getTopRatedTv(): Promise<MetadataRelayResult[]> {
    const data = await this.fetch<TmdbTvResponse>('/tv/top_rated', 'top-rated-tv')
    return data.results.map((t) => this.mapTv(t))
  }

  async getOnAirTv(): Promise<MetadataRelayResult[]> {
    const data = await this.fetch<TmdbTvResponse>('/tv/on_the_air', 'on-air-tv')
    return data.results.map((t) => this.mapTv(t))
  }

  async getTvDetails(tmdbId: string | number): Promise<TvSeriesDetails> {
    const data = await this.fetch<TmdbTvDetails>(`/tv/${tmdbId}`, `tv-details-${tmdbId}`)

    return {
      id: data.id.toString(),
      title: data.name,
      year: data.first_air_date ? new Date(data.first_air_date).getFullYear() : undefined,
      overview: data.overview,
      poster: data.poster_path || undefined,
      backdrop: data.backdrop_path || undefined,
      rating: data.vote_average,
      status: this.mapTmdbStatus(data.status),
      numberOfSeasons: data.number_of_seasons ?? 0,
      numberOfEpisodes: data.number_of_episodes ?? 0,
      seasons:
        data.seasons
          ?.filter((s) => s.season_number > 0)
          .map((s) => ({
            seasonNumber: s.season_number,
            name: s.name,
            episodeCount: s.episode_count ?? 0,
            airDate: s.air_date || undefined,
          })) ?? [],
      nextEpisodeToAir: data.next_episode_to_air
        ? {
            seasonNumber: data.next_episode_to_air.season_number,
            episodeNumber: data.next_episode_to_air.episode_number,
            name: data.next_episode_to_air.name,
            airDate: data.next_episode_to_air.air_date || undefined,
          }
        : undefined,
      lastEpisodeToAir: data.last_episode_to_air
        ? {
            seasonNumber: data.last_episode_to_air.season_number,
            episodeNumber: data.last_episode_to_air.episode_number,
            name: data.last_episode_to_air.name,
            airDate: data.last_episode_to_air.air_date || undefined,
          }
        : undefined,
    }
  }

  async getSeasonDetails(tmdbId: string | number, seasonNumber: number): Promise<SeasonDetails> {
    const data = await this.fetch<TmdbSeasonDetails>(
      `/tv/${tmdbId}/season/${seasonNumber}`,
      `season-details-${tmdbId}-${seasonNumber}`
    )

    return {
      seasonNumber: data.season_number,
      name: data.name,
      airDate: data.air_date || undefined,
      episodes: data.episodes.map((ep) => ({
        episodeNumber: ep.episode_number,
        seasonNumber: ep.season_number,
        name: ep.name,
        overview: ep.overview,
        airDate: ep.air_date || undefined,
      })),
    }
  }

  async getMovieDetails(tmdbId: string | number): Promise<MovieDetails> {
    const data = await this.fetch<TmdbMovieDetails>(`/movie/${tmdbId}`, `movie-details-${tmdbId}`)

    return {
      id: data.id.toString(),
      title: data.title,
      year: data.release_date ? new Date(data.release_date).getFullYear() : undefined,
      overview: data.overview,
      poster: data.poster_path || undefined,
      backdrop: data.backdrop_path || undefined,
      rating: data.vote_average,
      runtime: data.runtime,
      imdbId: data.imdb_id,
      collection: data.belongs_to_collection
        ? {
            id: data.belongs_to_collection.id,
            name: data.belongs_to_collection.name,
            posterPath: data.belongs_to_collection.poster_path || undefined,
            backdropPath: data.belongs_to_collection.backdrop_path || undefined,
          }
        : undefined,
    }
  }

  async getCollectionDetails(collectionId: number): Promise<CollectionDetails> {
    const data = await this.fetch<TmdbCollection>(
      `/collection/${collectionId}`,
      `collection-${collectionId}`
    )

    return {
      id: data.id,
      name: data.name,
      overview: data.overview,
      posterPath: data.poster_path || undefined,
      backdropPath: data.backdrop_path || undefined,
      movies: data.parts.map((part) => ({
        id: part.id,
        title: part.title,
        overview: part.overview,
        releaseDate: part.release_date,
        posterPath: part.poster_path || undefined,
      })),
    }
  }

  private mapTmdbStatus(status?: string): 'returning' | 'ended' | 'upcoming' | 'unknown' {
    if (!status) return 'unknown'
    const normalized = status.toLowerCase()
    if (normalized.includes('returning') || normalized.includes('continuing')) return 'returning'
    if (normalized.includes('ended') || normalized.includes('canceled')) return 'ended'
    if (normalized.includes('production') || normalized.includes('planned')) return 'upcoming'
    return 'unknown'
  }
}

// TMDB API types
type TmdbMovie = {
  id: number
  title: string
  release_date?: string
  overview?: string
  poster_path?: string | null
  backdrop_path?: string | null
  vote_average?: number
}

type TmdbTv = {
  id: number
  name: string
  first_air_date?: string
  overview?: string
  poster_path?: string | null
  backdrop_path?: string | null
  vote_average?: number
}

type TmdbMovieResponse = {
  results: TmdbMovie[]
  page: number
}

type TmdbTvResponse = {
  results: TmdbTv[]
  page: number
}

type TmdbEpisode = {
  id: number
  name: string
  overview?: string
  episode_number: number
  season_number: number
  air_date?: string | null
}

type TmdbSeason = {
  id: number
  name: string
  season_number: number
  episode_count?: number
  air_date?: string | null
}

type TmdbSeasonDetails = {
  id: number
  name: string
  season_number: number
  air_date?: string | null
  episodes: TmdbEpisode[]
}

type TmdbTvDetails = {
  id: number
  name: string
  overview?: string
  first_air_date?: string
  poster_path?: string | null
  backdrop_path?: string | null
  vote_average?: number
  status?: string
  number_of_seasons?: number
  number_of_episodes?: number
  seasons?: TmdbSeason[]
  next_episode_to_air?: TmdbEpisode | null
  last_episode_to_air?: TmdbEpisode | null
}

type TmdbMovieDetails = {
  id: number
  title: string
  overview?: string
  release_date?: string
  poster_path?: string | null
  backdrop_path?: string | null
  vote_average?: number
  runtime?: number
  imdb_id?: string
  belongs_to_collection?: {
    id: number
    name: string
    poster_path?: string | null
    backdrop_path?: string | null
  } | null
}

type TmdbCollection = {
  id: number
  name: string
  overview?: string
  poster_path?: string | null
  backdrop_path?: string | null
  parts: {
    id: number
    title: string
    overview?: string
    release_date?: string
    poster_path?: string | null
  }[]
}

// Orchestrates providers with priority-based fallback
export class MetadataService {
  private providers: MetadataProvider[]

  constructor(config: { relayUrl: string; tmdbApiKey?: string }) {
    this.providers = []

    // Add providers in priority order (relay first, then direct TMDB as fallback)
    this.providers.push(new RelayProvider(config.relayUrl))

    if (config.tmdbApiKey) {
      this.providers.push(new TmdbProvider(config.tmdbApiKey))
    }
  }

  private getProvider(capability: MetadataCapability): MetadataProvider | null {
    for (const provider of this.providers) {
      if (provider.capabilities.includes(capability)) {
        return provider
      }
    }
    return null
  }

  hasCapability(capability: MetadataCapability): boolean {
    return this.getProvider(capability) !== null
  }

  async searchMovie(query: string, year?: number): Promise<MetadataRelayResult[] | null> {
    const provider = this.getProvider('search')
    if (!provider?.searchMovie) return null
    return provider.searchMovie(query, year)
  }

  async searchTv(query: string, year?: number): Promise<MetadataRelayResult[] | null> {
    const provider = this.getProvider('search')
    if (!provider?.searchTv) return null
    return provider.searchTv(query, year)
  }

  async getTrendingMovies(): Promise<MetadataRelayResult[] | null> {
    const provider = this.getProvider('trending')
    if (!provider?.getTrendingMovies) return null
    return provider.getTrendingMovies()
  }

  async getTrendingTv(): Promise<MetadataRelayResult[] | null> {
    const provider = this.getProvider('trending')
    if (!provider?.getTrendingTv) return null
    return provider.getTrendingTv()
  }

  async getPopularMovies(): Promise<MetadataRelayResult[] | null> {
    const provider = this.getProvider('popular')
    if (!provider?.getPopularMovies) return null
    return provider.getPopularMovies()
  }

  async getPopularTv(): Promise<MetadataRelayResult[] | null> {
    const provider = this.getProvider('popular')
    if (!provider?.getPopularTv) return null
    return provider.getPopularTv()
  }

  async getUpcomingMovies(): Promise<MetadataRelayResult[] | null> {
    const provider = this.getProvider('upcoming')
    if (!provider?.getUpcomingMovies) return null
    return provider.getUpcomingMovies()
  }

  async getNowPlayingMovies(): Promise<MetadataRelayResult[] | null> {
    const provider = this.getProvider('nowPlaying')
    if (!provider?.getNowPlayingMovies) return null
    return provider.getNowPlayingMovies()
  }

  async getTopRatedMovies(): Promise<MetadataRelayResult[] | null> {
    const provider = this.getProvider('topRated')
    if (!provider?.getTopRatedMovies) return null
    return provider.getTopRatedMovies()
  }

  async getTopRatedTv(): Promise<MetadataRelayResult[] | null> {
    const provider = this.getProvider('topRated')
    if (!provider?.getTopRatedTv) return null
    return provider.getTopRatedTv()
  }

  async getOnAirTv(): Promise<MetadataRelayResult[] | null> {
    const provider = this.getProvider('onAir')
    if (!provider?.getOnAirTv) return null
    return provider.getOnAirTv()
  }

  async getTvDetails(tmdbId: string | number): Promise<TvSeriesDetails | null> {
    const provider = this.getProvider('tvDetails')
    if (!provider?.getTvDetails) return null
    return provider.getTvDetails(tmdbId)
  }

  async getSeasonDetails(
    tmdbId: string | number,
    seasonNumber: number
  ): Promise<SeasonDetails | null> {
    const provider = this.getProvider('seasonDetails')
    if (!provider?.getSeasonDetails) return null
    return provider.getSeasonDetails(tmdbId, seasonNumber)
  }

  async getMovieDetails(tmdbId: string | number): Promise<MovieDetails | null> {
    const provider = this.getProvider('movieDetails')
    if (!provider?.getMovieDetails) return null
    return provider.getMovieDetails(tmdbId)
  }

  async getCollectionDetails(collectionId: number): Promise<CollectionDetails | null> {
    const provider = this.getProvider('collectionDetails')
    if (!provider?.getCollectionDetails) return null
    return provider.getCollectionDetails(collectionId)
  }
}

// Singleton instance
let metadataService: MetadataService | null = null

export function getMetadataService(config: { relayUrl: string; tmdbApiKey?: string }): MetadataService {
  if (!metadataService) {
    metadataService = new MetadataService(config)
  }
  return metadataService
}
