import type { TmdbClient } from './tmdb'
import type { TvdbClient } from './tvdb'
import type { MetadataRelayClient } from './metadata-relay'

export type MetadataResult = {
  id: string
  title: string
  year?: number
  overview?: string
  posterPath?: string
  backdropPath?: string
  rating?: number
  provider: 'tmdb' | 'tvdb' | 'relay'
}

export type MetadataProviderConfig = {
  providers: Array<'relay' | 'tmdb' | 'tvdb'>
  tmdbApiKey?: string
  tvdbApiKey?: string
  relayUrl?: string
}

export class MetadataProvider {
  private clients: {
    tmdb?: TmdbClient
    tvdb?: TvdbClient
    relay?: MetadataRelayClient
  } = {}

  private providerOrder: Array<'relay' | 'tmdb' | 'tvdb'>

  constructor(
    config: MetadataProviderConfig,
    clients?: {
      tmdb?: TmdbClient
      tvdb?: TvdbClient
      relay?: MetadataRelayClient
    }
  ) {
    this.providerOrder = config.providers
    this.clients = clients || {}
  }

  async search(query: string, year?: number, type?: 'movie' | 'tv'): Promise<MetadataResult | null> {
    for (const provider of this.providerOrder) {
      try {
        const result = await this.searchWithProvider(provider, query, year, type)
        if (result) return result
      } catch (error) {
        console.error(`Failed to search with ${provider}:`, error)
      }
    }
    return null
  }

  private async searchWithProvider(
    provider: 'relay' | 'tmdb' | 'tvdb',
    query: string,
    year?: number,
    type?: 'movie' | 'tv'
  ): Promise<MetadataResult | null> {
    const client = this.clients[provider]
    if (!client) return null

    if (provider === 'relay') {
      const relayClient = client as MetadataRelayClient
      let results: Awaited<ReturnType<MetadataRelayClient['search']>>

      if (type === 'movie') {
        results = await relayClient.searchMovie(query, year)
      } else if (type === 'tv') {
        results = await relayClient.searchTv(query, year)
      } else {
        results = await relayClient.search(query, type)
      }

      if (results.length === 0) return null
      const first = results[0]
      if (!first) return null

      return {
        id: first.id,
        title: first.title,
        year: first.year,
        overview: first.overview,
        posterPath: first.poster,
        backdropPath: first.backdrop,
        rating: first.rating,
        provider: 'relay',
      }
    }

    if (provider === 'tmdb') {
      const tmdbClient = client as TmdbClient
      const searchMethod = type === 'movie' ? 'searchMovie' : type === 'tv' ? 'searchTv' : 'searchMulti'
      const result = await tmdbClient[searchMethod](query, year)

      if (result.results.length === 0) return null
      const first = result.results[0]
      if (!first) return null

      const releaseDate = 'release_date' in first ? first.release_date : first.first_air_date
      const extractedYear = releaseDate ? new Date(releaseDate).getFullYear() : undefined

      return {
        id: first.id.toString(),
        title: 'title' in first ? first.title : first.name,
        year: year || extractedYear,
        overview: first.overview || undefined,
        posterPath: first.poster_path || undefined,
        backdropPath: first.backdrop_path || undefined,
        rating: first.vote_average,
        provider: 'tmdb',
      }
    }

    if (provider === 'tvdb') {
      const tvdbClient = client as TvdbClient
      const results = await tvdbClient.search(query)

      if (results.length === 0) return null
      const first = results[0]
      if (!first) return null

      return {
        id: first.objectID,
        title: first.name,
        year: first.year ? Number.parseInt(first.year) : undefined,
        overview: first.overview,
        posterPath: first.image_url,
        provider: 'tvdb',
      }
    }

    return null
  }
}
