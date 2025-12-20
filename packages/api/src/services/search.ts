import { configService } from './config'
import { NewznabClient, type NewznabSearchParams } from '@jaeger/shared/clients/newznab'
import type { SearchResult, SearchRequest, SearchResponse, Indexer } from '@jaeger/shared/types/indexer'

interface SearchError {
  indexer: string
  error: string
}

export class SearchService {
  async search(request: SearchRequest): Promise<SearchResponse> {
    const indexers = configService.getEnabledIndexers()

    if (indexers.length === 0) {
      return { results: [], errors: [{ indexer: 'system', error: 'No indexers configured' }] }
    }

    const searchPromises = indexers.map((indexer) =>
      this.searchIndexer(indexer, request).catch((error) => ({
        results: [] as SearchResult[],
        error: {
          indexer: indexer.name,
          error: error instanceof Error ? error.message : 'Unknown error',
        },
      }))
    )

    const results = await Promise.all(searchPromises)

    const allResults: SearchResult[] = []
    const errors: SearchError[] = []

    for (const result of results) {
      if ('error' in result) {
        errors.push(result.error)
      } else {
        allResults.push(...result)
      }
    }

    // Sort by pub date (newest first)
    allResults.sort((a, b) => new Date(b.pubDate).getTime() - new Date(a.pubDate).getTime())

    return {
      results: allResults,
      errors: errors.length > 0 ? errors : undefined,
    }
  }

  private async searchIndexer(indexer: Indexer, request: SearchRequest): Promise<SearchResult[]> {
    if (indexer.protocol === 'newznab') {
      return this.searchNewznab(indexer, request)
    }

    throw new Error(`Unsupported protocol: ${indexer.protocol}`)
  }

  private async searchNewznab(indexer: Indexer, request: SearchRequest): Promise<SearchResult[]> {
    const client = new NewznabClient(indexer.url, indexer.apiKey)

    const params: NewznabSearchParams = {
      query: request.query,
      imdbId: request.imdbId,
      tvdbId: request.tvdbId,
      season: request.season,
      episode: request.episode,
    }

    // Add category filter based on search type
    if (request.type === 'movie' && indexer.categories?.movies) {
      params.categories = indexer.categories.movies
    } else if (request.type === 'tv' && indexer.categories?.tv) {
      params.categories = indexer.categories.tv
    } else if (request.categories) {
      params.categories = request.categories
    }

    return client.search(params, indexer.name, indexer.id)
  }
}

export const searchService = new SearchService()
