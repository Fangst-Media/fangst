import { mkdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { type ParsedRelease, parseReleaseName } from '@jaeger/parser'
import { SABnzbdClient } from '@jaeger/shared/clients/sabnzbd'
import type { SearchResult } from '@jaeger/shared/types/indexer'
import { configService } from '../services/config'
import { type RequestWithUser, requestService } from '../services/requests'
import { searchService } from '../services/search'

const QUALITY_ORDER = ['2160p', '1080p', '720p', '480p']

async function fetchRuntime(request: RequestWithUser): Promise<number | undefined> {
  if (!request.tmdbId) return undefined

  const relayUrl = configService.getMetadataRelayUrl()
  const endpoint =
    request.type === 'movie' ? `/tmdb/movies/${request.tmdbId}` : `/tmdb/tv/${request.tmdbId}`

  try {
    const response = await fetch(`${relayUrl}${endpoint}`, {
      headers: { 'User-Agent': 'Jaeger/0.1.0', Accept: 'application/json' },
    })
    if (!response.ok) return undefined

    const data = (await response.json()) as { runtime?: number; episode_run_time?: number[] }
    if (request.type === 'movie') {
      return data.runtime ?? undefined
    }
    // TV: use first episode_run_time if available
    return data.episode_run_time?.[0] ?? undefined
  } catch {
    return undefined
  }
}

interface ScoredResult {
  result: SearchResult
  parsed: ParsedRelease
}

function matchesQuality(parsed: ParsedRelease, minQuality?: string | null): boolean {
  if (!minQuality) return true

  const minIndex = QUALITY_ORDER.indexOf(minQuality)
  if (minIndex === -1) return true

  if (!parsed.resolution) return false

  const releaseIndex = QUALITY_ORDER.indexOf(parsed.resolution)
  // Higher index = lower quality. Accept if release quality >= minimum
  return releaseIndex !== -1 && releaseIndex <= minIndex
}

function selectBestRelease(
  results: SearchResult[],
  minQuality?: string | null
): SearchResult | null {
  // Parse all releases and calculate quality scores
  const scored: ScoredResult[] = results.map((result) => ({
    result,
    parsed: parseReleaseName(result.title),
  }))

  // Filter by minimum quality requirement
  const filtered = scored.filter(({ parsed }) => matchesQuality(parsed, minQuality))

  if (filtered.length === 0) return null

  // Sort by quality score (descending), then grabs as tiebreaker
  filtered.sort((a, b) => {
    const scoreA = a.parsed.qualityScore ?? 0
    const scoreB = b.parsed.qualityScore ?? 0

    if (scoreA !== scoreB) {
      return scoreB - scoreA // Higher score wins
    }

    // Tiebreaker: more grabs = more trusted
    const grabsA = a.result.grabs ?? 0
    const grabsB = b.result.grabs ?? 0
    if (grabsA !== grabsB) {
      return grabsB - grabsA
    }

    // Final tiebreaker: larger size
    return b.result.size - a.result.size
  })

  const best = filtered[0]
  if (best) {
    console.log(
      `[SearchJob] Selected: ${best.result.title} (score: ${best.parsed.qualityScore}, tier: ${best.parsed.releaseGroupTier ?? 'unknown'})`
    )
  }

  return best?.result ?? null
}

async function saveNzbBackup(nzbContent: string, filename: string): Promise<void> {
  const backupPath = configService.getNzbBackupPath()
  if (!backupPath) return

  try {
    await mkdir(backupPath, { recursive: true })
    const filePath = join(backupPath, filename.endsWith('.nzb') ? filename : `${filename}.nzb`)
    await writeFile(filePath, nzbContent, 'utf-8')
    console.log(`[SearchJob] NZB backed up to: ${filePath}`)
  } catch (error) {
    console.error('[SearchJob] Failed to save NZB backup:', error)
  }
}

async function downloadRelease(
  release: SearchResult,
  requestId: string,
  mediaType: 'movie' | 'tv'
): Promise<boolean> {
  const downloaders = configService.getEnabledDownloaders()

  if (downloaders.length === 0) {
    console.log('[SearchJob] No downloaders configured')
    return false
  }

  // Use first enabled downloader
  const downloader = downloaders[0]!

  if (downloader.type === 'sabnzbd') {
    const client = new SABnzbdClient(downloader.url, downloader.apiKey)
    try {
      await requestService.updateStatus(requestId, 'fetching_nzb')
      console.log(`[SearchJob] Fetching NZB: ${release.title}`)

      const category = mediaType === 'movie' ? downloader.categories?.movies : downloader.categories?.tv
      const result = await client.addNzbFromUrl(release.downloadUrl, release.title, category)

      // Save backup if configured
      if (result.nzbContent) {
        const safeFilename = release.title.replace(/[<>:"/\\|?*]/g, '_')
        await saveNzbBackup(result.nzbContent, safeFilename)
      }

      if (result.nzoId) {
        await requestService.updateDownloadInfo(requestId, result.nzoId, release.title)
        await requestService.updateStatus(requestId, 'downloading')
        console.log(`[SearchJob] Sent to ${downloader.name}: ${release.title} (nzoId: ${result.nzoId})`)
        return true
      }

      return false
    } catch (error) {
      console.error(`[SearchJob] Failed to send to ${downloader.name}:`, error)
      return false
    }
  }

  return false
}

async function processRequest(request: RequestWithUser): Promise<'found' | 'not_found' | 'error'> {
  console.log(`[SearchJob] Searching for: ${request.title} (${request.year ?? 'N/A'})`)

  try {
    const runtime = await fetchRuntime(request)

    const searchParams: {
      type: 'movie' | 'tv'
      imdbId?: string
      tvdbId?: number
      query?: string
      season?: number
      runtime?: number
    } = {
      type: request.type,
      runtime,
    }

    if (request.imdbId) {
      searchParams.imdbId = request.imdbId
    } else if (request.tvdbId) {
      searchParams.tvdbId = Number.parseInt(request.tvdbId, 10)
    } else {
      searchParams.query = request.year ? `${request.title} ${request.year}` : request.title
    }

    if (request.type === 'tv' && request.season) {
      searchParams.season = request.season
    }

    const response = await searchService.search(searchParams)

    if (response.results.length === 0) {
      console.log(`[SearchJob] No results for: ${request.title}`)
      await requestService.updateLastSearched(request.id)
      return 'not_found'
    }

    const bestRelease = selectBestRelease(response.results, request.minQuality)

    if (!bestRelease) {
      console.log(`[SearchJob] No releases matching quality for: ${request.title}`)
      await requestService.updateLastSearched(request.id)
      return 'not_found'
    }

    console.log(`[SearchJob] Found: ${bestRelease.title}`)
    await requestService.updateStatus(request.id, 'found')

    const downloaded = await downloadRelease(bestRelease, request.id, request.type)

    if (downloaded) {
      // For movies, mark as completed after download starts. For TV, keep monitoring
      if (request.type === 'movie') {
        await requestService.updateStatus(request.id, 'completed')
      }

      return 'found'
    }

    await requestService.updateStatus(request.id, 'failed')
    return 'error'
  } catch (error) {
    console.error(`[SearchJob] Error processing ${request.title}:`, error)
    return 'error'
  }
}

export async function runSearchJob(): Promise<{ searched: number; found: number; errors: number }> {
  console.log('[SearchJob] Starting search job...')

  const requests = await requestService.getApprovedForSearch()
  console.log(`[SearchJob] Found ${requests.length} requests to search`)

  let searched = 0
  let found = 0
  let errors = 0

  for (const request of requests) {
    // Update status to searching
    if (request.status === 'approved') {
      await requestService.updateStatus(request.id, 'searching')
    }

    const result = await processRequest(request)
    searched++

    if (result === 'found') {
      found++
    } else if (result === 'error') {
      errors++
    }

    // Small delay between requests to avoid hammering indexers
    await new Promise((resolve) => setTimeout(resolve, 1000))
  }

  console.log(`[SearchJob] Complete: searched=${searched}, found=${found}, errors=${errors}`)

  return { searched, found, errors }
}

// For manual triggering via API or testing
export { processRequest, selectBestRelease, matchesQuality }
