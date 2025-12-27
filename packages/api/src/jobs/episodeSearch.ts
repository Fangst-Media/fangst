import { mkdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { type ParsedRelease, parseReleaseName } from '@jaeger/parser'
import { SABnzbdClient } from '@jaeger/shared/clients/sabnzbd'
import type { SearchResult } from '@jaeger/shared/types/indexer'
import { configService } from '../services/config'
import { type RequestWithUser, requestService } from '../services/requests'
import type { Episode } from '../services/episodes'
import { searchService } from '../services/search'

const QUALITY_ORDER = ['2160p', '1080p', '720p', '480p']

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
  return releaseIndex !== -1 && releaseIndex <= minIndex
}

function selectBestRelease(results: SearchResult[], minQuality?: string | null): SearchResult | null {
  const scored: ScoredResult[] = results.map((result) => ({
    result,
    parsed: parseReleaseName(result.title),
  }))

  const filtered = scored.filter(({ parsed }) => matchesQuality(parsed, minQuality))
  if (filtered.length === 0) return null

  filtered.sort((a, b) => {
    const scoreA = a.parsed.qualityScore ?? 0
    const scoreB = b.parsed.qualityScore ?? 0
    if (scoreA !== scoreB) return scoreB - scoreA
    const grabsA = a.result.grabs ?? 0
    const grabsB = b.result.grabs ?? 0
    if (grabsA !== grabsB) return grabsB - grabsA
    return b.result.size - a.result.size
  })

  return filtered[0]?.result ?? null
}

function isSeasonPack(title: string): boolean {
  const parsed = parseReleaseName(title)
  // Season pack = has season but no episode number, or explicitly "complete"
  return parsed.season !== undefined && parsed.episode === undefined
}

async function saveNzbBackup(nzbContent: string, filename: string): Promise<void> {
  const backupPath = configService.getNzbBackupPath()
  if (!backupPath) return

  try {
    await mkdir(backupPath, { recursive: true })
    const filePath = join(backupPath, filename.endsWith('.nzb') ? filename : `${filename}.nzb`)
    await writeFile(filePath, nzbContent, 'utf-8')
    console.log(`[EpisodeSearch] NZB backed up to: ${filePath}`)
  } catch (error) {
    console.error('[EpisodeSearch] Failed to save NZB backup:', error)
  }
}

async function downloadRelease(
  release: SearchResult,
  releaseName: string
): Promise<{ success: boolean; nzoId?: string }> {
  const downloaders = configService.getEnabledDownloaders()

  if (downloaders.length === 0) {
    console.log('[EpisodeSearch] No downloaders configured')
    return { success: false }
  }

  const downloader = downloaders[0]!

  if (downloader.type === 'sabnzbd') {
    const client = new SABnzbdClient(downloader.url, downloader.apiKey)
    try {
      const category = downloader.categories?.tv
      const result = await client.addNzbFromUrl(release.downloadUrl, releaseName, category)

      if (result.nzbContent) {
        const safeFilename = releaseName.replace(/[<>:"/\\|?*]/g, '_')
        await saveNzbBackup(result.nzbContent, safeFilename)
      }

      if (result.nzoId) {
        console.log(`[EpisodeSearch] Sent to ${downloader.name}: ${releaseName} (nzoId: ${result.nzoId})`)
        return { success: true, nzoId: result.nzoId }
      }

      return { success: false }
    } catch (error) {
      console.error(`[EpisodeSearch] Failed to send to ${downloader.name}:`, error)
      return { success: false }
    }
  }

  return { success: false }
}

async function searchForSeasonPack(
  request: RequestWithUser
): Promise<SearchResult | null> {
  if (!request.season) return null

  console.log(`[EpisodeSearch] Searching for season pack: ${request.title} S${String(request.season).padStart(2, '0')}`)

  const searchParams: {
    type: 'movie' | 'tv'
    imdbId?: string
    tvdbId?: number
    query?: string
    season?: number
  } = {
    type: 'tv',
    season: request.season,
  }

  if (request.imdbId) {
    searchParams.imdbId = request.imdbId
  } else if (request.tvdbId) {
    searchParams.tvdbId = Number.parseInt(request.tvdbId, 10)
  } else {
    searchParams.query = request.year ? `${request.title} ${request.year}` : request.title
  }

  try {
    const response = await searchService.search(searchParams)

    // Filter to only season packs
    const seasonPacks = response.results.filter((r) => isSeasonPack(r.title))

    if (seasonPacks.length === 0) {
      console.log(`[EpisodeSearch] No season packs found for: ${request.title} S${request.season}`)
      return null
    }

    const best = selectBestRelease(seasonPacks, request.minQuality)
    if (best) {
      console.log(`[EpisodeSearch] Found season pack: ${best.title}`)
    }

    return best
  } catch (error) {
    console.error(`[EpisodeSearch] Error searching for season pack:`, error)
    return null
  }
}

async function searchForEpisode(
  request: RequestWithUser,
  episode: Episode
): Promise<SearchResult | null> {
  const epCode = `S${String(episode.seasonNumber).padStart(2, '0')}E${String(episode.episodeNumber).padStart(2, '0')}`
  console.log(`[EpisodeSearch] Searching for: ${request.title} ${epCode}`)

  const searchParams: {
    type: 'movie' | 'tv'
    imdbId?: string
    tvdbId?: number
    query?: string
    season?: number
    episode?: number
  } = {
    type: 'tv',
    season: episode.seasonNumber,
    episode: episode.episodeNumber,
  }

  if (request.imdbId) {
    searchParams.imdbId = request.imdbId
  } else if (request.tvdbId) {
    searchParams.tvdbId = Number.parseInt(request.tvdbId, 10)
  } else {
    searchParams.query = request.year ? `${request.title} ${request.year}` : request.title
  }

  try {
    const response = await searchService.search(searchParams)

    if (response.results.length === 0) {
      console.log(`[EpisodeSearch] No results for: ${request.title} ${epCode}`)
      return null
    }

    const best = selectBestRelease(response.results, request.minQuality)
    if (best) {
      console.log(`[EpisodeSearch] Found: ${best.title}`)
    }

    return best
  } catch (error) {
    console.error(`[EpisodeSearch] Error searching for episode:`, error)
    return null
  }
}

interface EpisodeSearchResult {
  checked: number
  found: number
  errors: number
  seasonPacks: number
}

export async function runEpisodeSearchJob(): Promise<EpisodeSearchResult> {
  console.log('[EpisodeSearch] Starting episode search job...')

  const stats = {
    checked: 0,
    found: 0,
    errors: 0,
    seasonPacks: 0,
  }

  // Get TV requests with pending episodes
  const tvRequests = await requestService.getTvRequestsWithPendingEpisodes()
  console.log(`[EpisodeSearch] Found ${tvRequests.length} TV requests with pending episodes`)

  for (const request of tvRequests) {
    if (!request.season) continue

    const pendingEpisodes = await requestService.getPendingEpisodesByRequest(request.id)
    const allEpisodes = await requestService.getEpisodes(request.id)

    if (pendingEpisodes.length === 0) continue

    // Determine if we should search for season pack
    const pendingRatio = pendingEpisodes.length / allEpisodes.length
    const shouldTrySeasonPack = pendingRatio >= 0.5 && allEpisodes.every((ep) => ep.status === 'pending' || ep.status === 'failed')

    if (shouldTrySeasonPack) {
      // Try to find a season pack first
      const seasonPack = await searchForSeasonPack(request)

      if (seasonPack) {
        const { success, nzoId } = await downloadRelease(seasonPack, seasonPack.title)

        if (success) {
          // Mark all pending episodes in this season as downloading
          await requestService.updateAllEpisodesForSeason(
            request.id,
            request.season,
            'downloading',
            nzoId,
            seasonPack.title
          )

          stats.seasonPacks++
          stats.found += pendingEpisodes.length
          stats.checked += pendingEpisodes.length
          console.log(`[EpisodeSearch] Season pack downloaded, marked ${pendingEpisodes.length} episodes as downloading`)

          // Small delay before next request
          await new Promise((resolve) => setTimeout(resolve, 1000))
          continue
        }
      }
    }

    // No season pack found or doesn't apply, search for individual episodes
    // Only search for episodes that have aired (airDate <= today or airDate is null)
    const today = new Date().toISOString().split('T')[0]!
    const airedEpisodes = pendingEpisodes.filter((ep) => !ep.airDate || ep.airDate <= today)

    for (const episode of airedEpisodes) {
      stats.checked++

      // Mark as searching
      await requestService.updateEpisodeStatus(episode.id, 'searching')

      const result = await searchForEpisode(request, episode)

      if (result) {
        const { success, nzoId } = await downloadRelease(result, result.title)

        if (success) {
          await requestService.updateEpisodeStatus(episode.id, 'downloading', nzoId, result.title)
          stats.found++
        } else {
          await requestService.updateEpisodeStatus(episode.id, 'failed')
          stats.errors++
        }
      } else {
        // No result found, set back to pending for next search
        await requestService.updateEpisodeStatus(episode.id, 'pending')
      }

      // Small delay between episodes to avoid hammering indexers
      await new Promise((resolve) => setTimeout(resolve, 500))
    }

    // Small delay between requests
    await new Promise((resolve) => setTimeout(resolve, 1000))
  }

  console.log(
    `[EpisodeSearch] Complete: checked=${stats.checked}, found=${stats.found}, seasonPacks=${stats.seasonPacks}, errors=${stats.errors}`
  )

  return stats
}

export { searchForSeasonPack, searchForEpisode, isSeasonPack }
