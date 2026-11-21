import { and, eq, sql } from 'drizzle-orm'
import { db } from '../db'
import { episodes, requests } from '../db/schema'

export type EpisodeStatus = 'pending' | 'searching' | 'downloading' | 'completed' | 'failed'

export type CreateEpisodeData = {
  seasonNumber: number
  episodeNumber: number
  title?: string
  airDate?: string
}

export type Episode = {
  id: string
  requestId: string
  seasonNumber: number
  episodeNumber: number
  title: string | null
  airDate: string | null
  status: EpisodeStatus
  monitored: boolean
  nzoId: string | null
  releaseName: string | null
  createdAt: Date
  updatedAt: Date
}

export type MonitoringPreset = 'all' | 'future' | 'missing' | 'latest_season'

export type EpisodeWithRequest = Episode & {
  request: {
    id: string
    title: string
    tmdbId: string | null
    season: number | null
  }
}

function generateId(): string {
  return crypto.randomUUID()
}

export class EpisodeService {
  async createForSeason(requestId: string, episodeList: CreateEpisodeData[]): Promise<Episode[]> {
    return this.createWithPreset(requestId, episodeList, 'all')
  }

  async createWithPreset(
    requestId: string,
    episodeList: CreateEpisodeData[],
    preset: MonitoringPreset
  ): Promise<Episode[]> {
    const now = new Date()
    const today = new Date().toISOString().split('T')[0]!
    const created: Episode[] = []

    const highestSeason = Math.max(...episodeList.map((ep) => ep.seasonNumber))

    for (const ep of episodeList) {
      const id = generateId()
      const monitored = this.shouldMonitorEpisode(ep, preset, today, highestSeason)

      await db.insert(episodes).values({
        id,
        requestId,
        seasonNumber: ep.seasonNumber,
        episodeNumber: ep.episodeNumber,
        title: ep.title ?? null,
        airDate: ep.airDate ?? null,
        status: 'pending',
        monitored,
        createdAt: now,
        updatedAt: now,
      })

      created.push({
        id,
        requestId,
        seasonNumber: ep.seasonNumber,
        episodeNumber: ep.episodeNumber,
        title: ep.title ?? null,
        airDate: ep.airDate ?? null,
        status: 'pending',
        monitored,
        nzoId: null,
        releaseName: null,
        createdAt: now,
        updatedAt: now,
      })
    }

    await this.updateRequestEpisodeCounts(requestId)
    return created
  }

  private shouldMonitorEpisode(
    ep: CreateEpisodeData,
    preset: MonitoringPreset,
    today: string,
    highestSeason: number
  ): boolean {
    switch (preset) {
      case 'all':
        return true
      case 'future':
        return ep.airDate ? ep.airDate > today : true
      case 'missing':
        return true
      case 'latest_season':
        return ep.seasonNumber === highestSeason
      default:
        return true
    }
  }

  async getByRequestId(requestId: string): Promise<Episode[]> {
    const results = await db.select().from(episodes).where(eq(episodes.requestId, requestId))

    return results.map((ep) => ({
      id: ep.id,
      requestId: ep.requestId,
      seasonNumber: ep.seasonNumber,
      episodeNumber: ep.episodeNumber,
      title: ep.title,
      airDate: ep.airDate,
      status: ep.status as EpisodeStatus,
      monitored: ep.monitored,
      nzoId: ep.nzoId,
      releaseName: ep.releaseName,
      createdAt: ep.createdAt,
      updatedAt: ep.updatedAt,
    }))
  }

  async getById(id: string): Promise<Episode | null> {
    const results = await db.select().from(episodes).where(eq(episodes.id, id)).limit(1)
    if (results.length === 0) return null

    const ep = results[0]!
    return {
      id: ep.id,
      requestId: ep.requestId,
      seasonNumber: ep.seasonNumber,
      episodeNumber: ep.episodeNumber,
      title: ep.title,
      airDate: ep.airDate,
      status: ep.status as EpisodeStatus,
      monitored: ep.monitored,
      nzoId: ep.nzoId,
      releaseName: ep.releaseName,
      createdAt: ep.createdAt,
      updatedAt: ep.updatedAt,
    }
  }

  async setMonitored(episodeId: string, monitored: boolean): Promise<void> {
    await db
      .update(episodes)
      .set({ monitored, updatedAt: new Date() })
      .where(eq(episodes.id, episodeId))
  }

  async updateStatus(
    episodeId: string,
    status: EpisodeStatus,
    nzoId?: string,
    releaseName?: string
  ): Promise<void> {
    const updates: Record<string, unknown> = {
      status,
      updatedAt: new Date(),
    }

    if (nzoId !== undefined) updates.nzoId = nzoId
    if (releaseName !== undefined) updates.releaseName = releaseName

    await db.update(episodes).set(updates).where(eq(episodes.id, episodeId))

    const ep = await this.getById(episodeId)
    if (ep) {
      await this.updateRequestEpisodeCounts(ep.requestId)
    }
  }

  async updateAllForSeason(
    requestId: string,
    seasonNumber: number,
    status: EpisodeStatus,
    nzoId?: string,
    releaseName?: string
  ): Promise<void> {
    const updates: Record<string, unknown> = {
      status,
      updatedAt: new Date(),
    }

    if (nzoId !== undefined) updates.nzoId = nzoId
    if (releaseName !== undefined) updates.releaseName = releaseName

    await db
      .update(episodes)
      .set(updates)
      .where(and(eq(episodes.requestId, requestId), eq(episodes.seasonNumber, seasonNumber)))

    await this.updateRequestEpisodeCounts(requestId)
  }

  async getReadyToSearch(): Promise<EpisodeWithRequest[]> {
    const today = new Date().toISOString().split('T')[0]!

    const results = await db
      .select({
        episode: episodes,
        request: {
          id: requests.id,
          title: requests.title,
          tmdbId: requests.tmdbId,
          season: requests.season,
        },
      })
      .from(episodes)
      .innerJoin(requests, eq(episodes.requestId, requests.id))
      .where(
        and(
          eq(episodes.status, 'pending'),
          eq(episodes.monitored, true),
          sql`(${episodes.airDate} IS NULL OR ${episodes.airDate} <= ${today})`
        )
      )

    return results.map((row) => ({
      id: row.episode.id,
      requestId: row.episode.requestId,
      seasonNumber: row.episode.seasonNumber,
      episodeNumber: row.episode.episodeNumber,
      title: row.episode.title,
      airDate: row.episode.airDate,
      status: row.episode.status as EpisodeStatus,
      monitored: row.episode.monitored,
      nzoId: row.episode.nzoId,
      releaseName: row.episode.releaseName,
      createdAt: row.episode.createdAt,
      updatedAt: row.episode.updatedAt,
      request: row.request,
    }))
  }

  async getPendingByRequest(requestId: string): Promise<Episode[]> {
    const allEps = await this.getByRequestId(requestId)
    return allEps.filter((ep) => ep.status === 'pending')
  }

  async addOne(requestId: string, episode: CreateEpisodeData, monitored = true): Promise<Episode> {
    const now = new Date()
    const id = generateId()

    await db.insert(episodes).values({
      id,
      requestId,
      seasonNumber: episode.seasonNumber,
      episodeNumber: episode.episodeNumber,
      title: episode.title ?? null,
      airDate: episode.airDate ?? null,
      status: 'pending',
      monitored,
      createdAt: now,
      updatedAt: now,
    })

    await this.updateRequestEpisodeCounts(requestId)

    return {
      id,
      requestId,
      seasonNumber: episode.seasonNumber,
      episodeNumber: episode.episodeNumber,
      title: episode.title ?? null,
      airDate: episode.airDate ?? null,
      status: 'pending',
      monitored,
      nzoId: null,
      releaseName: null,
      createdAt: now,
      updatedAt: now,
    }
  }

  async updateRequestEpisodeCounts(requestId: string): Promise<void> {
    const allEps = await this.getByRequestId(requestId)
    const totalEpisodes = allEps.length
    const downloadedEpisodes = allEps.filter((ep) => ep.status === 'completed').length

    await db
      .update(requests)
      .set({ totalEpisodes, downloadedEpisodes })
      .where(eq(requests.id, requestId))
  }

  async updateRequestSeriesStatus(
    requestId: string,
    seriesStatus: 'returning' | 'ended' | 'upcoming' | 'unknown'
  ): Promise<void> {
    await db.update(requests).set({ seriesStatus }).where(eq(requests.id, requestId))
  }
}

export const episodeService = new EpisodeService()
