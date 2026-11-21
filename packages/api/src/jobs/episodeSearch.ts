import { parseReleaseName } from '@fangst/parser'
import type { SearchResult } from '@fangst/shared/types/indexer'
import { logger } from '../lib/logger'
import { configService } from '../services/config'
import { downloadService } from '../services/downloads'
import type { Episode } from '../services/episodes'
import { profileService } from '../services/profiles'
import { type RequestWithUser, requestService } from '../services/requests'
import { searchService } from '../services/search'
import { selectBestRelease } from './searchRequests'

function isSeasonPack(title: string): boolean {
  const parsed = parseReleaseName(title)
  return parsed.season !== undefined && parsed.episode === undefined
}

async function searchForSeasonPack(request: RequestWithUser): Promise<SearchResult | null> {
  if (!request.season) return null

  logger.info({ title: request.title, season: request.season }, 'Searching for season pack')

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
    const seasonPacks = response.results.filter((r) => isSeasonPack(r.title))

    if (seasonPacks.length === 0) return null

    const profile = request.qualityProfileId
      ? await profileService.getById(request.qualityProfileId)
      : await profileService.getDefault()

    return selectBestRelease(seasonPacks, profile, request.minQuality)
  } catch (error) {
    logger.error({ error }, 'Error searching for season pack')
    return null
  }
}

async function searchForEpisode(
  request: RequestWithUser,
  episode: Episode
): Promise<SearchResult | null> {
  const epCode = `S${String(episode.seasonNumber).padStart(2, '0')}E${String(episode.episodeNumber).padStart(2, '0')}`
  logger.info({ title: request.title, episode: epCode }, 'Searching for episode')

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
    if (response.results.length === 0) return null

    const profile = request.qualityProfileId
      ? await profileService.getById(request.qualityProfileId)
      : await profileService.getDefault()

    return selectBestRelease(response.results, profile, request.minQuality)
  } catch (error) {
    logger.error({ error }, 'Error searching for episode')
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
  logger.info('Starting episode search job')

  const stats = { checked: 0, found: 0, errors: 0, seasonPacks: 0 }

  const tvRequests = await requestService.getTvRequestsWithPendingEpisodes()
  logger.info({ count: tvRequests.length }, 'TV requests with pending episodes')

  for (const request of tvRequests) {
    if (!request.season) continue

    const pendingEpisodes = await requestService.getPendingEpisodesByRequest(request.id)
    const allEpisodes = await requestService.getEpisodes(request.id)
    if (pendingEpisodes.length === 0) continue

    const pendingRatio = pendingEpisodes.length / allEpisodes.length
    const shouldTrySeasonPack =
      pendingRatio >= 0.5 &&
      allEpisodes.every((ep) => ep.status === 'pending' || ep.status === 'failed')

    if (shouldTrySeasonPack) {
      const seasonPack = await searchForSeasonPack(request)

      if (seasonPack) {
        try {
          const download = await downloadService.createSeasonPack(request.id, {
            releaseName: seasonPack.title,
            downloadUrl: seasonPack.downloadUrl,
            indexer: seasonPack.indexer,
            size: seasonPack.size,
          })

          const downloaders = configService.getEnabledDownloaders()
          const category = downloaders[0]?.categories?.tv
          const { nzoId } = await downloadService.sendToDownloader(
            download.id,
            seasonPack.downloadUrl,
            category
          )

          // Mark all pending episodes as downloading (legacy compat)
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

          await new Promise((resolve) => setTimeout(resolve, 1000))
          continue
        } catch (error) {
          logger.error({ error, title: request.title }, 'Failed to download season pack')
        }
      }
    }

    // Individual episode search
    const today = new Date().toISOString().split('T')[0]!
    const airedEpisodes = pendingEpisodes.filter((ep) => !ep.airDate || ep.airDate <= today)

    for (const episode of airedEpisodes) {
      stats.checked++
      await requestService.updateEpisodeStatus(episode.id, 'searching')

      const result = await searchForEpisode(request, episode)

      if (result) {
        try {
          const download = await downloadService.createForEpisode(request.id, episode.id, {
            releaseName: result.title,
            downloadUrl: result.downloadUrl,
            indexer: result.indexer,
            size: result.size,
          })

          const downloaders = configService.getEnabledDownloaders()
          const category = downloaders[0]?.categories?.tv
          const { nzoId } = await downloadService.sendToDownloader(
            download.id,
            result.downloadUrl,
            category
          )

          // Legacy compat
          await requestService.updateEpisodeStatus(episode.id, 'downloading', nzoId, result.title)
          stats.found++
        } catch (error) {
          logger.error({ error }, 'Failed to download episode')
          await requestService.updateEpisodeStatus(episode.id, 'failed')
          stats.errors++
        }
      } else {
        await requestService.updateEpisodeStatus(episode.id, 'pending')
      }

      await new Promise((resolve) => setTimeout(resolve, 500))
    }

    await new Promise((resolve) => setTimeout(resolve, 1000))
  }

  logger.info(stats, 'Episode search job complete')
  return stats
}

export { searchForSeasonPack, searchForEpisode, isSeasonPack }
