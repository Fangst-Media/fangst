import { parseReleaseName } from '@fangst/parser'
import { NewznabClient } from '@fangst/shared/clients/newznab'
import type { Indexer, SearchResult } from '@fangst/shared/types/indexer'
import { logger } from '../lib/logger'
import { configService } from './config'

export type IndexerTrendingItem = {
  title: string
  grabs: number
  category: string | undefined
  size: number
  pubDate: string
  indexer: string
  indexerId: string
  parsedTitle?: string
  year?: number
}

export type IndexerTrendingResult = {
  results: IndexerTrendingItem[]
  indexerCount: number
}

const DEFAULT_MOVIE_CATEGORIES = [2000, 2010, 2020, 2030, 2040, 2050, 2060]
const DEFAULT_TV_CATEGORIES = [5000, 5010, 5020, 5030, 5040, 5050, 5060]

function getCategories(indexer: Indexer, mediaType: 'movie' | 'tv'): number[] {
  if (mediaType === 'movie') {
    return indexer.categories?.movies?.map(Number) ?? DEFAULT_MOVIE_CATEGORIES
  }
  return indexer.categories?.tv?.map(Number) ?? DEFAULT_TV_CATEGORIES
}

async function fetchFromIndexer(
  indexer: Indexer,
  mediaType: 'movie' | 'tv'
): Promise<IndexerTrendingItem[]> {
  const client = new NewznabClient({
    baseUrl: indexer.url,
    apiKey: indexer.apiKey,
    apiPath: indexer.apiPath,
  })

  const categories = getCategories(indexer, mediaType)

  const results: SearchResult[] = await client.search(
    { categories, limit: 50 },
    indexer.name,
    indexer.id
  )

  return results
    .filter((r) => r.grabs !== undefined && r.grabs > 0)
    .sort((a, b) => (b.grabs ?? 0) - (a.grabs ?? 0))
    .slice(0, 20)
    .map((r) => {
      const parsed = parseReleaseName(r.title)
      return {
        title: r.title,
        grabs: r.grabs ?? 0,
        category: r.category,
        size: r.size,
        pubDate: r.pubDate,
        indexer: indexer.name,
        indexerId: indexer.id,
        parsedTitle: parsed.title,
        year: parsed.year,
      }
    })
}

export class IndexerTrendingService {
  async getTrending(mediaType: 'movie' | 'tv', indexerId?: string): Promise<IndexerTrendingResult> {
    const indexers = configService.getEnabledIndexers()

    if (indexers.length === 0) {
      return { results: [], indexerCount: 0 }
    }

    const targetIndexers = indexerId ? indexers.filter((i) => i.id === indexerId) : indexers

    const promises = targetIndexers.map((indexer) =>
      fetchFromIndexer(indexer, mediaType).catch((err) => {
        logger.warn(
          { indexer: indexer.name, error: err instanceof Error ? err.message : err },
          'Failed to fetch indexer trending'
        )
        return [] as IndexerTrendingItem[]
      })
    )

    const allResults = (await Promise.all(promises)).flat()

    // Dedup by parsed title similarity and sum grabs across indexers
    const deduped = deduplicateByTitle(allResults)
    deduped.sort((a, b) => b.grabs - a.grabs)

    return {
      results: deduped.slice(0, 20),
      indexerCount: targetIndexers.length,
    }
  }

  async getPerIndexerTrending(
    mediaType: 'movie' | 'tv'
  ): Promise<Record<string, IndexerTrendingItem[]>> {
    const indexers = configService.getEnabledIndexers()
    const result: Record<string, IndexerTrendingItem[]> = {}

    const promises = indexers.map(async (indexer) => {
      try {
        const items = await fetchFromIndexer(indexer, mediaType)
        result[indexer.name] = items
      } catch (err) {
        logger.warn({ indexer: indexer.name, error: err }, 'Failed to fetch indexer trending')
        result[indexer.name] = []
      }
    })

    await Promise.all(promises)
    return result
  }
}

function deduplicateByTitle(items: IndexerTrendingItem[]): IndexerTrendingItem[] {
  const seen = new Map<string, IndexerTrendingItem>()

  for (const item of items) {
    const key = (item.parsedTitle ?? item.title).toLowerCase().replace(/[^a-z0-9]/g, '')
    const existing = seen.get(key)
    if (existing) {
      existing.grabs += item.grabs
    } else {
      seen.set(key, { ...item })
    }
  }

  return Array.from(seen.values())
}

export const indexerTrendingService = new IndexerTrendingService()
