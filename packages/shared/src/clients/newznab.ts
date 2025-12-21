import type { SearchResult, IndexerTestResult } from '../types/indexer'

export interface NewznabSearchParams {
  query?: string
  imdbId?: string
  tvdbId?: number
  season?: number
  episode?: number
  categories?: number[]
  limit?: number
  offset?: number
}

export interface NewznabClientOptions {
  baseUrl: string
  apiKey: string
  apiPath?: string
}

export class NewznabClient {
  private baseUrl: string
  private apiKey: string
  private apiPath: string

  constructor(options: NewznabClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, '')
    this.apiKey = options.apiKey
    this.apiPath = options.apiPath ?? '/api'
  }

  private buildUrl(mode: string, params: Record<string, string | number | undefined> = {}): string {
    const url = new URL(`${this.baseUrl}${this.apiPath}`)
    url.searchParams.set('apikey', this.apiKey)
    url.searchParams.set('o', 'json')
    url.searchParams.set('t', mode)

    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined) {
        url.searchParams.set(key, String(value))
      }
    }

    return url.toString()
  }

  async testConnection(): Promise<IndexerTestResult> {
    try {
      const url = this.buildUrl('caps')
      const response = await fetch(url)

      if (!response.ok) {
        return {
          success: false,
          error: `HTTP ${response.status}: ${response.statusText}`,
        }
      }

      const text = await response.text()

      // Caps endpoint returns XML even with o=json
      if (text.includes('<error')) {
        const errorMatch = text.match(/description="([^"]+)"/)
        return {
          success: false,
          error: errorMatch?.[1] ?? 'Unknown API error',
        }
      }

      // Extract version from caps response
      const versionMatch = text.match(/version="([^"]+)"/)
      const version = versionMatch?.[1]

      return {
        success: true,
        version,
      }
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
      }
    }
  }

  async search(
    params: NewznabSearchParams,
    indexerName: string,
    indexerId: string
  ): Promise<SearchResult[]> {
    const { query, imdbId, tvdbId, season, episode, categories, limit = 100, offset = 0 } = params

    // Determine search type based on params
    let mode = 'search'
    const searchParams: Record<string, string | number | undefined> = {
      limit,
      offset,
    }

    if (imdbId) {
      mode = 'movie'
      // IMDB IDs should be numeric only (strip 'tt' prefix if present)
      searchParams.imdbid = imdbId.replace(/^tt/, '')
    } else if (tvdbId !== undefined) {
      mode = 'tvsearch'
      searchParams.tvdbid = tvdbId
      if (season !== undefined) searchParams.season = season
      if (episode !== undefined) searchParams.ep = episode
    } else if (query) {
      searchParams.q = query
    }

    if (categories && categories.length > 0) {
      searchParams.cat = categories.join(',')
    }

    const url = this.buildUrl(mode, searchParams)
    const response = await fetch(url)

    if (!response.ok) {
      throw new Error(`Newznab search failed: ${response.statusText}`)
    }

    const text = await response.text()

    // Check for XML error response
    if (text.includes('<error')) {
      const errorMatch = text.match(/description="([^"]+)"/)
      throw new Error(errorMatch?.[1] ?? 'Unknown API error')
    }

    return this.parseResponse(text, indexerName, indexerId)
  }

  private parseResponse(text: string, indexerName: string, indexerId: string): SearchResult[] {
    const results: SearchResult[] = []

    // Try JSON parsing first (some indexers support it)
    try {
      const json = JSON.parse(text)
      if (json.channel?.item || json.item) {
        const items = json.channel?.item || json.item
        const itemArray = Array.isArray(items) ? items : [items]

        for (const item of itemArray) {
          if (!item) continue

          const result = this.parseJsonItem(item, indexerName, indexerId)
          if (result) results.push(result)
        }

        return results
      }
    } catch {
      // Not JSON, parse as XML
    }

    // Parse XML/RSS response
    const itemMatches = text.matchAll(/<item>([\s\S]*?)<\/item>/g)

    for (const match of itemMatches) {
      const itemXml = match[1]
      if (!itemXml) continue

      const result = this.parseXmlItem(itemXml, indexerName, indexerId)
      if (result) results.push(result)
    }

    return results
  }

  private parseJsonItem(
    item: Record<string, unknown>,
    indexerName: string,
    indexerId: string
  ): SearchResult | null {
    // Handle both direct attrs and @attributes nested format
    const rawAttrs = item.attr as Array<{ '@attributes'?: { name: string; value: string }; name?: string; value?: string }> | undefined
    const attrs = rawAttrs?.map(a => a['@attributes'] ?? a) as Array<{ name: string; value: string }> | undefined
    const guid = (item.guid as string) || attrs?.find((a) => a.name === 'guid')?.value
    const title = item.title as string
    const enclosure = item.enclosure as Record<string, unknown> | undefined
    const enclosureAttrs = enclosure?.['@attributes'] as Record<string, string> | undefined
    const downloadUrl = (item.link as string) || enclosureAttrs?.url

    if (!guid || !title || !downloadUrl) return null

    // Extract size from attributes
    let size = 0
    if (attrs) {
      const sizeAttr = attrs.find((a) => a.name === 'size')
      if (sizeAttr) size = Number.parseInt(sizeAttr.value, 10)
    }
    if (!size && item.size) size = Number.parseInt(item.size as string, 10)

    // Extract grabs
    let grabs: number | undefined
    if (attrs) {
      const grabsAttr = attrs.find((a) => a.name === 'grabs')
      if (grabsAttr) grabs = Number.parseInt(grabsAttr.value, 10)
    }

    return {
      guid,
      title,
      indexer: indexerName,
      indexerId,
      downloadUrl,
      size: size || 0,
      pubDate: (item.pubDate as string) || new Date().toISOString(),
      category: item.category as string | undefined,
      grabs,
    }
  }

  private parseXmlItem(
    itemXml: string,
    indexerName: string,
    indexerId: string
  ): SearchResult | null {
    const getTag = (tag: string): string | undefined => {
      const match = itemXml.match(new RegExp(`<${tag}><!\\[CDATA\\[([^\\]]*?)\\]\\]><\\/${tag}>|<${tag}>([^<]*)<\\/${tag}>`))
      return match?.[1] || match?.[2]
    }

    const guid = getTag('guid')
    const title = getTag('title')

    // Get download URL from enclosure
    const enclosureMatch = itemXml.match(/url="([^"]+)".*?type="application\/x-nzb"/)
    const downloadUrl = enclosureMatch?.[1]

    if (!guid || !title || !downloadUrl) return null

    // Get size from enclosure or newznab:attr
    let size = 0
    const enclosureSizeMatch = itemXml.match(/length="(\d+)"/)
    if (enclosureSizeMatch) {
      size = Number.parseInt(enclosureSizeMatch[1]!, 10)
    }

    const sizeAttrMatch = itemXml.match(/newznab:attr name="size" value="(\d+)"/)
    if (sizeAttrMatch) {
      size = Number.parseInt(sizeAttrMatch[1]!, 10)
    }

    // Get grabs from newznab:attr
    let grabs: number | undefined
    const grabsMatch = itemXml.match(/newznab:attr name="grabs" value="(\d+)"/)
    if (grabsMatch) {
      grabs = Number.parseInt(grabsMatch[1]!, 10)
    }

    // Get category
    const category = getTag('category')

    // Get pubDate
    const pubDate = getTag('pubDate') || new Date().toISOString()

    return {
      guid,
      title,
      indexer: indexerName,
      indexerId,
      downloadUrl,
      size,
      pubDate,
      category,
      grabs,
    }
  }
}
