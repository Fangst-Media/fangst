import { configService } from './config'
import { type MediaItemWithMetadata, mediaService } from './media'
import { getMetadataService } from './metadata'

export type TvSeriesOverview = {
  tmdbId: string
  title: string
  year?: number
  posterPath?: string
  backdropPath?: string
  rating?: number
  episodeCount: number
  seasonNumbers: number[]
}

export type SeasonProgress = {
  seasonNumber: number
  name: string
  episodeCount: number
  downloadedCount: number
  airDate?: string
}

export type SeriesWithSeasons = {
  series: {
    tmdbId: string
    title: string
    year?: number
    overview?: string
    posterPath?: string
    backdropPath?: string
    rating?: number
    status: 'returning' | 'ended' | 'upcoming' | 'unknown'
    numberOfSeasons: number
  }
  seasons: SeasonProgress[]
  totalDownloaded: number
  totalEpisodes: number
}

export type EpisodeFile = {
  id: string
  filename: string
  resolution?: string
  size?: number
  videoCodec?: string
  audioCodec?: string
  hdr?: string
  source?: string
}

export type EpisodeWithFile = {
  episodeNumber: number
  title?: string
  overview?: string
  airDate?: string
  downloaded: boolean
  file?: EpisodeFile
}

export type SeasonWithEpisodes = {
  seasonNumber: number
  name: string
  airDate?: string
  episodes: EpisodeWithFile[]
}

export class LibraryService {
  async getTvSeries(): Promise<TvSeriesOverview[]> {
    const tvItems = await mediaService.getTvShows()

    // Group by tmdb ID (from metadata) or by parsed title
    const seriesMap = new Map<string, MediaItemWithMetadata[]>()

    for (const item of tvItems) {
      const key = item.metadata?.id ?? `parsed:${item.parsed.title.toLowerCase()}`
      const existing = seriesMap.get(key) ?? []
      existing.push(item)
      seriesMap.set(key, existing)
    }

    const results: TvSeriesOverview[] = []

    for (const [key, items] of seriesMap) {
      const firstItem = items[0]!
      const seasonNumbers = [
        ...new Set(items.map((i) => i.parsed.season).filter((s): s is number => s !== undefined)),
      ]

      const tmdbId = key.startsWith('parsed:') ? undefined : key.replace(/^[^:]+:/, '')

      if (tmdbId) {
        results.push({
          tmdbId,
          title: firstItem.metadata?.title ?? firstItem.parsed.title,
          year: firstItem.metadata?.year ?? firstItem.parsed.year,
          posterPath: firstItem.metadata?.posterPath,
          backdropPath: firstItem.metadata?.backdropPath,
          rating: firstItem.metadata?.rating,
          episodeCount: items.length,
          seasonNumbers: seasonNumbers.sort((a, b) => a - b),
        })
      }
    }

    return results.sort((a, b) => a.title.localeCompare(b.title))
  }

  async getTvSeriesById(tmdbId: string): Promise<SeriesWithSeasons | null> {
    const tvItems = await mediaService.getTvShows()

    // Filter items for this series
    const seriesItems = tvItems.filter((item) => {
      const itemTmdbId = item.metadata?.id?.replace(/^[^:]+:/, '')
      return itemTmdbId === tmdbId
    })

    if (seriesItems.length === 0) {
      return null
    }

    // Fetch fresh TMDB data
    const metadataService = getMetadataService({
      relayUrl: configService.getMetadataRelayUrl(),
      tmdbApiKey: configService.getTmdbApiKey(),
    })

    const tvDetails = await metadataService.getTvDetails(tmdbId)
    if (!tvDetails) {
      return null
    }

    // Group downloaded items by season
    const downloadedBySeason = new Map<number, MediaItemWithMetadata[]>()
    for (const item of seriesItems) {
      if (item.parsed.season !== undefined) {
        const existing = downloadedBySeason.get(item.parsed.season) ?? []
        existing.push(item)
        downloadedBySeason.set(item.parsed.season, existing)
      }
    }

    // Build season progress
    const seasons: SeasonProgress[] = tvDetails.seasons.map((season) => {
      const downloaded = downloadedBySeason.get(season.seasonNumber) ?? []
      return {
        seasonNumber: season.seasonNumber,
        name: season.name,
        episodeCount: season.episodeCount,
        downloadedCount: downloaded.length,
        airDate: season.airDate,
      }
    })

    const totalDownloaded = seriesItems.length
    const totalEpisodes = tvDetails.seasons.reduce((sum, s) => sum + s.episodeCount, 0)

    return {
      series: {
        tmdbId,
        title: tvDetails.title,
        year: tvDetails.year,
        overview: tvDetails.overview,
        posterPath: tvDetails.poster,
        backdropPath: tvDetails.backdrop,
        rating: tvDetails.rating,
        status: tvDetails.status,
        numberOfSeasons: tvDetails.numberOfSeasons,
      },
      seasons,
      totalDownloaded,
      totalEpisodes,
    }
  }

  async getSeasonEpisodes(
    tmdbId: string,
    seasonNumber: number
  ): Promise<SeasonWithEpisodes | null> {
    const tvItems = await mediaService.getTvShows()

    // Filter items for this series and season
    const seasonItems = tvItems.filter((item) => {
      const itemTmdbId = item.metadata?.id?.replace(/^[^:]+:/, '')
      return itemTmdbId === tmdbId && item.parsed.season === seasonNumber
    })

    // Create map of downloaded episodes
    const downloadedByEpisode = new Map<number, MediaItemWithMetadata>()
    for (const item of seasonItems) {
      if (item.parsed.episode !== undefined) {
        downloadedByEpisode.set(item.parsed.episode, item)
      }
    }

    // Fetch season details from TMDB
    const metadataService = getMetadataService({
      relayUrl: configService.getMetadataRelayUrl(),
      tmdbApiKey: configService.getTmdbApiKey(),
    })

    const seasonDetails = await metadataService.getSeasonDetails(tmdbId, seasonNumber)
    if (!seasonDetails) {
      return null
    }

    // Build episode list with file info
    const episodes: EpisodeWithFile[] = seasonDetails.episodes.map((ep) => {
      const downloaded = downloadedByEpisode.get(ep.episodeNumber)

      return {
        episodeNumber: ep.episodeNumber,
        title: ep.name,
        overview: ep.overview,
        airDate: ep.airDate,
        downloaded: !!downloaded,
        file: downloaded
          ? {
              id: downloaded.id,
              filename: downloaded.filename,
              resolution: downloaded.parsed.resolution,
              size: downloaded.size,
              videoCodec: downloaded.parsed.videoCodec,
              audioCodec: downloaded.parsed.audioCodec,
              hdr: downloaded.parsed.hdr,
              source: downloaded.parsed.source,
            }
          : undefined,
      }
    })

    return {
      seasonNumber: seasonDetails.seasonNumber,
      name: seasonDetails.name,
      airDate: seasonDetails.airDate,
      episodes,
    }
  }
}

export const libraryService = new LibraryService()
