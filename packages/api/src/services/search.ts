import { parseReleaseName } from '@fangst/parser'
import { NewznabClient, type NewznabSearchParams } from '@fangst/shared/clients/newznab'
import { isReleaseTooSmall } from '@fangst/shared/lib/size-filter'
import type {
  Indexer,
  ParsedReleaseInfo,
  SearchRequest,
  SearchResponse,
  SearchResult,
} from '@fangst/shared/types/indexer'
import { configService } from './config'

interface SearchError {
  indexer: string
  error: string
}

interface SearchResultWithPriority extends SearchResult {
  indexerPriority: number
}

export class SearchService {
  async search(request: SearchRequest): Promise<SearchResponse> {
    const indexers = configService.getEnabledIndexers()

    if (indexers.length === 0) {
      return { results: [], errors: [{ indexer: 'system', error: 'No indexers configured' }] }
    }

    const searchPromises = indexers.map((indexer) =>
      this.searchIndexer(indexer, request)
        .then((results) => ({
          results: results.map((r) => ({ ...r, indexerPriority: indexer.priority ?? 50 })),
        }))
        .catch((error) => ({
          results: [] as SearchResultWithPriority[],
          error: {
            indexer: indexer.name,
            error: error instanceof Error ? error.message : 'Unknown error',
          },
        }))
    )

    const results = await Promise.all(searchPromises)

    const allResults: SearchResultWithPriority[] = []
    const errors: SearchError[] = []

    for (const result of results) {
      if ('error' in result) {
        errors.push(result.error)
      }
      allResults.push(...result.results)
    }

    // Parse each result to extract quality info
    const parsedResults = allResults.map((result) => {
      const parsed = parseReleaseName(result.title)
      const parsedInfo: ParsedReleaseInfo = {
        resolution: parsed.resolution,
        source: parsed.source,
        videoCodec: parsed.videoCodec,
        audioCodec: parsed.audioCodec,
        hdr: parsed.hdr,
        releaseGroup: parsed.releaseGroup,
        releaseGroupTier: parsed.releaseGroupTier,
        streamingService: parsed.streamingService,
        isRemux: parsed.isRemux,
        proper: parsed.proper,
        repack: parsed.repack,
        qualityScore: parsed.qualityScore,
      }
      return { ...result, parsed: parsedInfo }
    })

    // Filter out releases that are too small based on MB/min calculation
    const sizeFilterConfig = configService.get().sizeFilter
    const filteredResults = request.runtime
      ? parsedResults.filter(
          (result) =>
            !isReleaseTooSmall(
              result.size,
              request.runtime!,
              result.parsed?.resolution,
              sizeFilterConfig
            )
        )
      : parsedResults

    // Sort by quality score (highest first), then by indexer priority (lower = better), then by pub date
    filteredResults.sort((a, b) => {
      const scoreA = a.parsed?.qualityScore ?? 0
      const scoreB = b.parsed?.qualityScore ?? 0
      if (scoreA !== scoreB) return scoreB - scoreA
      const priorityA = (a as SearchResultWithPriority).indexerPriority ?? 50
      const priorityB = (b as SearchResultWithPriority).indexerPriority ?? 50
      if (priorityA !== priorityB) return priorityA - priorityB
      return new Date(b.pubDate).getTime() - new Date(a.pubDate).getTime()
    })

    return {
      results: filteredResults,
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
    const client = new NewznabClient({
      baseUrl: indexer.url,
      apiKey: indexer.apiKey,
      apiPath: indexer.apiPath,
    })

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
