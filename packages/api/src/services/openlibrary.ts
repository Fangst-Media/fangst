import { logger } from '../lib/logger'
import type { BookDetails, MetadataCapability, MetadataProvider } from './metadata'

type OpenLibraryWork = {
  key: string
  title: string
  author_name?: string[]
  author_key?: string[]
  cover_i?: number
  cover_edition_key?: string
  first_publish_year?: number
  edition_count?: number
}

type OpenLibraryWorkDetails = {
  key: string
  title: string
  description?: string | { value: string }
  covers?: number[]
  subjects?: string[]
  first_publish_date?: string
}

import type { MetadataRelayResult } from '@fangst/shared/clients/metadata-relay'

type MetadataResult = MetadataRelayResult

function getCoverUrl(coverId: number, size: 'S' | 'M' | 'L' = 'L'): string {
  return `https://covers.openlibrary.org/b/id/${coverId}-${size}.jpg`
}

function extractWorkId(key: string): string {
  return key.replace('/works/', '')
}

function extractDescription(desc?: string | { value: string }): string | undefined {
  if (!desc) return undefined
  if (typeof desc === 'string') return desc
  return desc.value
}

function mapWork(work: OpenLibraryWork): MetadataResult {
  return {
    id: extractWorkId(work.key),
    title: work.title,
    year: work.first_publish_year,
    poster: work.cover_i ? getCoverUrl(work.cover_i, 'L') : undefined,
    author: work.author_name?.[0],
    type: 'book',
  }
}

export class OpenLibraryProvider implements MetadataProvider {
  name = 'openlibrary'
  capabilities: MetadataCapability[] = [
    'bookSearch',
    'bookTrending',
    'bookPopular',
    'bookClassics',
    'bookDetails',
  ]

  private baseUrl = 'https://openlibrary.org'
  private cache = new Map<string, { data: unknown; expires: number }>()
  private cacheTtl = 24 * 60 * 60 * 1000

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

  private async fetch<T>(path: string, cacheKey?: string): Promise<T> {
    const key = cacheKey ?? path
    const cached = this.getCached<T>(key)
    if (cached) return cached

    const url = `${this.baseUrl}${path}`
    const response = await fetch(url, {
      headers: { Accept: 'application/json' },
    })

    if (!response.ok) {
      throw new Error(`Open Library API error: ${response.statusText}`)
    }

    const data = await response.json()
    this.setCache(key, data)
    return data as T
  }

  async searchBooks(query: string): Promise<MetadataResult[]> {
    const data = await this.fetch<{ docs: OpenLibraryWork[] }>(
      `/search.json?q=${encodeURIComponent(query)}&limit=20&fields=key,title,author_name,author_key,cover_i,first_publish_year,edition_count`,
      `search:${query}`
    )
    return data.docs.map(mapWork)
  }

  async getTrendingBooks(): Promise<MetadataResult[]> {
    const data = await this.fetch<{ works: OpenLibraryWork[] }>('/trending/daily.json', 'trending')
    return data.works.slice(0, 20).map(mapWork)
  }

  async getPopularBooks(): Promise<MetadataResult[]> {
    const data = await this.fetch<{ works: OpenLibraryWork[] }>('/trending/weekly.json', 'popular')
    return data.works.slice(0, 20).map(mapWork)
  }

  async getClassicBooks(): Promise<MetadataResult[]> {
    const data = await this.fetch<{ works: OpenLibraryWork[] }>(
      '/subjects/classic_literature.json?limit=20',
      'classics'
    )
    return data.works.slice(0, 20).map(mapWork)
  }

  async getBookDetails(workId: string): Promise<BookDetails> {
    const data = await this.fetch<OpenLibraryWorkDetails>(
      `/works/${workId}.json`,
      `details:${workId}`
    )

    const coverId = data.covers?.[0]
    let author: string | undefined
    let authorId: string | undefined

    // Fetch author info from the search endpoint since work details don't include it
    try {
      const searchData = await this.fetch<{ docs: OpenLibraryWork[] }>(
        `/search.json?q=${encodeURIComponent(data.title)}&limit=1&fields=author_name,author_key`,
        `author-lookup:${workId}`
      )
      author = searchData.docs[0]?.author_name?.[0]
      authorId = searchData.docs[0]?.author_key?.[0]
    } catch (error) {
      logger.warn({ workId, error }, 'Failed to fetch author info')
    }

    return {
      id: extractWorkId(data.key),
      title: data.title,
      author,
      authorId,
      year: data.first_publish_date
        ? Number.parseInt(data.first_publish_date, 10) || undefined
        : undefined,
      cover: coverId ? getCoverUrl(coverId, 'L') : undefined,
      overview: extractDescription(data.description),
      subjects: data.subjects?.slice(0, 10),
    }
  }
}
