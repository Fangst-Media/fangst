import { type ParsedRelease, parseReleaseName } from '@fangst/parser'
import type { SearchResult } from '@fangst/shared/types/indexer'
import type { QualityProfile } from '@fangst/shared/types/profiles'
import { logger } from '../lib/logger'
import { meetsCutoff, scoreReleaseWithProfile } from '../lib/profile-scoring'
import { configService } from '../services/config'
import { downloadService } from '../services/downloads'
import { profileService } from '../services/profiles'
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
      headers: { 'User-Agent': 'Fangst/0.1.0', Accept: 'application/json' },
    })
    if (!response.ok) return undefined

    const data = (await response.json()) as { runtime?: number; episode_run_time?: number[] }
    if (request.type === 'movie') {
      return data.runtime ?? undefined
    }
    return data.episode_run_time?.[0] ?? undefined
  } catch {
    return undefined
  }
}

function matchesQuality(parsed: ParsedRelease, minQuality?: string | null): boolean {
  if (!minQuality) return true
  const minIndex = QUALITY_ORDER.indexOf(minQuality)
  if (minIndex === -1) return true
  if (!parsed.resolution) return false
  const releaseIndex = QUALITY_ORDER.indexOf(parsed.resolution)
  return releaseIndex !== -1 && releaseIndex <= minIndex
}

/**
 * Select best release using a quality profile (preferred) or legacy minQuality fallback.
 */
export function selectBestRelease(
  results: SearchResult[],
  profile: QualityProfile | null,
  minQuality?: string | null
): SearchResult | null {
  const scored = results.map((result) => ({
    result,
    parsed: parseReleaseName(result.title),
  }))

  if (profile) {
    const withScores = scored.map(({ result, parsed }) => {
      const score = scoreReleaseWithProfile(parsed, profile)
      return { result, parsed, ...score }
    })

    const eligible = withScores.filter((s) => !s.rejected)
    if (eligible.length === 0) return null

    eligible.sort((a, b) => {
      if (a.totalScore !== b.totalScore) return b.totalScore - a.totalScore
      const grabsA = a.result.grabs ?? 0
      const grabsB = b.result.grabs ?? 0
      if (grabsA !== grabsB) return grabsB - grabsA
      return b.result.size - a.result.size
    })

    const best = eligible[0]
    if (best) {
      logger.info(
        {
          release: best.result.title,
          score: best.totalScore,
          base: best.baseScore,
          bonus: best.profileBonus,
        },
        'Selected release with profile'
      )
    }
    return best?.result ?? null
  }

  // Legacy fallback: filter by minQuality, sort by qualityScore
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

  const best = filtered[0]
  if (best) {
    logger.info(
      { release: best.result.title, score: best.parsed.qualityScore },
      'Selected release (legacy)'
    )
  }
  return best?.result ?? null
}

async function processRequest(request: RequestWithUser): Promise<'found' | 'not_found' | 'error'> {
  logger.info({ title: request.title, year: request.year }, 'Searching for request')

  try {
    const runtime = await fetchRuntime(request)

    // Load quality profile
    const profile = request.qualityProfileId
      ? await profileService.getById(request.qualityProfileId)
      : await profileService.getDefault()

    // Check if this is an upgrade search
    if (request.status === ('completed' as string)) {
      const bestDownload = await downloadService.getBestComplete(request.id)
      if (bestDownload && profile && meetsCutoff(bestDownload.qualityScore ?? 0, profile)) {
        logger.info(
          { title: request.title, score: bestDownload.qualityScore, cutoff: profile.cutoffScore },
          'Already meets cutoff, skipping'
        )
        return 'not_found'
      }
    }

    const searchParams: {
      type: 'movie' | 'tv' | 'book' | 'general'
      imdbId?: string
      tvdbId?: number
      query?: string
      season?: number
      runtime?: number
      categories?: number[]
    } = {
      type: request.type,
      runtime,
    }

    if (request.type === 'book') {
      const authorSuffix = request.author ? ` ${request.author}` : ''
      searchParams.query = `${request.title}${authorSuffix}`
      searchParams.categories = request.format === 'audiobook' ? [3030] : [7020]
      searchParams.runtime = undefined
    } else if (request.imdbId) {
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
      logger.info({ title: request.title }, 'No results found')
      await requestService.updateLastSearched(request.id)
      return 'not_found'
    }

    const bestRelease = selectBestRelease(response.results, profile, request.minQuality)

    if (!bestRelease) {
      logger.info({ title: request.title }, 'No releases matching quality')
      await requestService.updateLastSearched(request.id)
      return 'not_found'
    }

    logger.info({ title: request.title, release: bestRelease.title }, 'Found release')

    // Create download via DownloadService
    const download = await downloadService.createForRequest(request.id, {
      releaseName: bestRelease.title,
      downloadUrl: bestRelease.downloadUrl,
      indexer: bestRelease.indexer,
      size: bestRelease.size,
    })

    // Determine category for downloader
    const downloaders = configService.getEnabledDownloaders()
    const downloader = downloaders[0]
    const category =
      request.type === 'movie'
        ? downloader?.categories?.movies
        : request.type === 'book'
          ? request.format === 'audiobook'
            ? downloader?.categories?.audiobooks
            : downloader?.categories?.books
          : downloader?.categories?.tv

    try {
      await downloadService.sendToDownloader(download.id, bestRelease.downloadUrl, category)

      // If this is an upgrade, mark old downloads as replaced
      if (request.status === ('completed' as string)) {
        const existingDownloads = await downloadService.getByRequestId(request.id)
        for (const existing of existingDownloads) {
          if (existing.id !== download.id && existing.status === 'complete') {
            await downloadService.markReplaced(existing.id, download.id)
          }
        }
      }

      await requestService.updateStatus(request.id, 'downloading')
      return 'found'
    } catch (error) {
      logger.error({ error, title: request.title }, 'Failed to send to downloader')
      await downloadService.updateStatus(
        download.id,
        'failed',
        error instanceof Error ? error.message : 'Send failed'
      )
      await requestService.updateStatus(request.id, 'failed')
      return 'error'
    }
  } catch (error) {
    logger.error({ error, title: request.title }, 'Error processing request')
    return 'error'
  }
}

export async function runSearchJob(): Promise<{ searched: number; found: number; errors: number }> {
  logger.info('Starting search job')

  const pendingRequests = await requestService.getApprovedForSearch()

  // Also check fulfilled requests eligible for upgrade
  const allRequests = await requestService.getAll()
  const upgradeEligible = allRequests.filter((r) => {
    if (r.status !== 'completed') return false
    if (!r.qualityProfileId) return false
    return r.mode === 'auto'
  })

  const requests = [...pendingRequests, ...upgradeEligible]
  logger.info(
    { count: requests.length, pending: pendingRequests.length, upgrades: upgradeEligible.length },
    'Requests to search'
  )

  let searched = 0
  let found = 0
  let errors = 0

  for (const request of requests) {
    if (request.status === 'approved') {
      await requestService.updateStatus(request.id, 'searching')
    }

    const result = await processRequest(request)
    searched++

    if (result === 'found') found++
    else if (result === 'error') errors++

    // Small delay between requests to avoid hammering indexers
    await new Promise((resolve) => setTimeout(resolve, 1000))
  }

  logger.info({ searched, found, errors }, 'Search job complete')
  return { searched, found, errors }
}

export { processRequest, matchesQuality }
