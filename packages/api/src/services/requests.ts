import { eq, inArray, and, isNull, sql } from 'drizzle-orm'
import { db } from '../db'
import { requests, users, requestUsers, episodes } from '../db/schema'

export type RequestStatus =
  | 'pending_approval'
  | 'approved'
  | 'searching'
  | 'found'
  | 'fetching_nzb'
  | 'nzb_downloaded'
  | 'sent_to_downloader'
  | 'downloading'
  | 'download_complete'
  | 'importing'
  | 'completed'
  | 'rejected'
  | 'failed'

export type CreateRequestData = {
  title: string
  year?: number
  type: 'movie' | 'tv'
  tmdbId?: string
  imdbId?: string
  tvdbId?: string
  season?: number
  posterPath?: string
  overview?: string
  minQuality?: string
  // TV series specific
  seriesStatus?: 'returning' | 'ended' | 'upcoming' | 'unknown'
  totalEpisodes?: number
}

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
  nzoId: string | null
  releaseName: string | null
  createdAt: Date
  updatedAt: Date
}

export type EpisodeWithRequest = Episode & {
  request: {
    id: string
    title: string
    tmdbId: string | null
    season: number | null
  }
}

export type RequestingUser = {
  id: string
  name: string
  requestedAt: Date
}

export type RequestWithUser = {
  id: string
  title: string
  year: number | null
  type: 'movie' | 'tv'
  tmdbId: string | null
  imdbId: string | null
  tvdbId: string | null
  season: number | null
  posterPath: string | null
  overview: string | null
  status: RequestStatus
  minQuality: string | null
  nzoId: string | null
  releaseName: string | null
  // TV series specific
  seriesStatus: 'returning' | 'ended' | 'upcoming' | 'unknown' | null
  totalEpisodes: number | null
  downloadedEpisodes: number | null
  // Timestamps
  createdAt: Date
  approvedAt: Date | null
  lastSearchedAt: Date | null
  completedAt: Date | null
  requestedBy: RequestingUser
  requestedByUsers: RequestingUser[]
  approvedBy: {
    id: string
    name: string
  } | null
}

function generateId(): string {
  return crypto.randomUUID()
}

export class RequestService {
  private async findExistingRequest(data: CreateRequestData): Promise<string | null> {
    // Find existing request for same media (by tmdbId or title+year+type+season)
    if (data.tmdbId) {
      const existing = await db
        .select({ id: requests.id })
        .from(requests)
        .where(
          and(
            eq(requests.tmdbId, data.tmdbId),
            eq(requests.type, data.type),
            data.season !== undefined ? eq(requests.season, data.season) : isNull(requests.season)
          )
        )
        .limit(1)
      if (existing.length > 0) return existing[0]!.id
    }
    return null
  }

  private async addUserToRequest(requestId: string, userId: string): Promise<boolean> {
    // Check if user already added to this request
    const existing = await db
      .select({ id: requestUsers.id })
      .from(requestUsers)
      .where(and(eq(requestUsers.requestId, requestId), eq(requestUsers.userId, userId)))
      .limit(1)

    if (existing.length > 0) return false

    await db.insert(requestUsers).values({
      id: generateId(),
      requestId,
      userId,
      createdAt: new Date(),
    })
    return true
  }

  async create(
    data: CreateRequestData,
    userId: string,
    isAdmin: boolean
  ): Promise<RequestWithUser> {
    const now = new Date()

    // Check for existing request for same media
    const existingId = await this.findExistingRequest(data)
    if (existingId) {
      await this.addUserToRequest(existingId, userId)
      const existing = await this.getById(existingId)
      if (!existing) throw new Error('Failed to find existing request')
      return existing
    }

    const id = generateId()

    // Admins auto-approve, members need approval
    const status: RequestStatus = isAdmin ? 'approved' : 'pending_approval'

    await db.insert(requests).values({
      id,
      title: data.title,
      year: data.year ?? null,
      type: data.type,
      tmdbId: data.tmdbId ?? null,
      imdbId: data.imdbId ?? null,
      tvdbId: data.tvdbId ?? null,
      season: data.season ?? null,
      posterPath: data.posterPath ?? null,
      overview: data.overview ?? null,
      minQuality: data.minQuality ?? null,
      // TV series specific
      seriesStatus: data.seriesStatus ?? null,
      totalEpisodes: data.totalEpisodes ?? null,
      requestedBy: userId,
      status,
      createdAt: now,
      approvedAt: isAdmin ? now : null,
      approvedBy: isAdmin ? userId : null,
    })

    // Add user to join table
    await this.addUserToRequest(id, userId)

    const created = await this.getById(id)
    if (!created) {
      throw new Error('Failed to create request')
    }
    return created
  }

  async getAll(filters?: {
    status?: RequestStatus | RequestStatus[]
    userId?: string
    type?: 'movie' | 'tv'
  }): Promise<RequestWithUser[]> {
    const results = await db
      .select({
        request: requests,
        requestedByUser: users,
      })
      .from(requests)
      .leftJoin(users, eq(requests.requestedBy, users.id))

    // Get all requesting users from join table
    const requestIds = results.map((r) => r.request.id)
    const allRequestUsers =
      requestIds.length > 0
        ? await db
            .select({
              requestId: requestUsers.requestId,
              userId: requestUsers.userId,
              createdAt: requestUsers.createdAt,
              userName: users.name,
            })
            .from(requestUsers)
            .leftJoin(users, eq(requestUsers.userId, users.id))
            .where(inArray(requestUsers.requestId, requestIds))
        : []

    // Group requesting users by request ID
    const requestUsersMap = new Map<string, RequestingUser[]>()
    for (const ru of allRequestUsers) {
      const list = requestUsersMap.get(ru.requestId) ?? []
      list.push({
        id: ru.userId,
        name: ru.userName ?? 'Unknown',
        requestedAt: ru.createdAt,
      })
      requestUsersMap.set(ru.requestId, list)
    }

    // Get approved by users separately
    const approvedByIds = results
      .map((r) => r.request.approvedBy)
      .filter((id): id is string => id !== null)

    const approvedByUsers =
      approvedByIds.length > 0
        ? await db.select().from(users).where(inArray(users.id, approvedByIds))
        : []

    const approvedByMap = new Map(approvedByUsers.map((u) => [u.id, u]))

    let filtered = results

    if (filters?.status) {
      const statuses = Array.isArray(filters.status) ? filters.status : [filters.status]
      filtered = filtered.filter((r) => statuses.includes(r.request.status as RequestStatus))
    }

    if (filters?.userId) {
      // Filter by user in join table instead of just requestedBy
      const userRequestIds = new Set(
        allRequestUsers.filter((ru) => ru.userId === filters.userId).map((ru) => ru.requestId)
      )
      filtered = filtered.filter((r) => userRequestIds.has(r.request.id))
    }

    if (filters?.type) {
      filtered = filtered.filter((r) => r.request.type === filters.type)
    }

    return filtered.map((row) => {
      const requestingUsers = requestUsersMap.get(row.request.id) ?? []
      // Sort by request date, earliest first
      requestingUsers.sort((a, b) => a.requestedAt.getTime() - b.requestedAt.getTime())
      const primaryUser = requestingUsers[0] ?? {
        id: row.requestedByUser?.id ?? row.request.requestedBy,
        name: row.requestedByUser?.name ?? 'Unknown',
        requestedAt: row.request.createdAt,
      }

      return {
        id: row.request.id,
        title: row.request.title,
        year: row.request.year,
        type: row.request.type as 'movie' | 'tv',
        tmdbId: row.request.tmdbId,
        imdbId: row.request.imdbId,
        tvdbId: row.request.tvdbId,
        season: row.request.season,
        posterPath: row.request.posterPath,
        overview: row.request.overview,
        status: row.request.status as RequestStatus,
        minQuality: row.request.minQuality,
        nzoId: row.request.nzoId,
        releaseName: row.request.releaseName,
        seriesStatus: row.request.seriesStatus as 'returning' | 'ended' | 'upcoming' | 'unknown' | null,
        totalEpisodes: row.request.totalEpisodes,
        downloadedEpisodes: row.request.downloadedEpisodes,
        createdAt: row.request.createdAt,
        approvedAt: row.request.approvedAt,
        lastSearchedAt: row.request.lastSearchedAt,
        completedAt: row.request.completedAt,
        requestedBy: primaryUser,
        requestedByUsers: requestingUsers,
        approvedBy: row.request.approvedBy
          ? {
              id: row.request.approvedBy,
              name: approvedByMap.get(row.request.approvedBy)?.name ?? 'Unknown',
            }
          : null,
      }
    })
  }

  async getById(id: string): Promise<RequestWithUser | null> {
    const results = await this.getAll()
    return results.find((r) => r.id === id) ?? null
  }

  async delete(id: string, userId: string, isAdmin: boolean): Promise<void> {
    const request = await this.getById(id)
    if (!request) {
      throw new Error('Request not found')
    }

    if (isAdmin) {
      // Admins can delete the entire request
      await db.delete(requestUsers).where(eq(requestUsers.requestId, id))
      await db.delete(requests).where(eq(requests.id, id))
      return
    }

    // Non-admins can only remove themselves from the request
    const isRequester = request.requestedByUsers.some((u) => u.id === userId)
    if (!isRequester) {
      throw new Error('Not authorized to modify this request')
    }

    // Remove user from request
    await db
      .delete(requestUsers)
      .where(and(eq(requestUsers.requestId, id), eq(requestUsers.userId, userId)))

    // If no users left, delete the request entirely
    const remainingUsers = await db
      .select({ id: requestUsers.id })
      .from(requestUsers)
      .where(eq(requestUsers.requestId, id))
      .limit(1)

    if (remainingUsers.length === 0) {
      await db.delete(requests).where(eq(requests.id, id))
    }
  }

  async approve(id: string, adminId: string): Promise<RequestWithUser> {
    const request = await this.getById(id)
    if (!request) {
      throw new Error('Request not found')
    }

    if (request.status !== 'pending_approval') {
      throw new Error('Request is not pending approval')
    }

    await db
      .update(requests)
      .set({
        status: 'approved',
        approvedAt: new Date(),
        approvedBy: adminId,
      })
      .where(eq(requests.id, id))

    const updated = await this.getById(id)
    if (!updated) {
      throw new Error('Failed to approve request')
    }
    return updated
  }

  async reject(id: string, adminId: string): Promise<RequestWithUser> {
    const request = await this.getById(id)
    if (!request) {
      throw new Error('Request not found')
    }

    if (request.status !== 'pending_approval') {
      throw new Error('Request is not pending approval')
    }

    await db
      .update(requests)
      .set({
        status: 'rejected',
        approvedBy: adminId,
      })
      .where(eq(requests.id, id))

    const updated = await this.getById(id)
    if (!updated) {
      throw new Error('Failed to reject request')
    }
    return updated
  }

  async updateStatus(id: string, status: RequestStatus): Promise<void> {
    const updates: Record<string, unknown> = { status }

    if (status === 'completed') {
      updates.completedAt = new Date()
    }

    if (status === 'searching') {
      updates.lastSearchedAt = new Date()
    }

    await db.update(requests).set(updates).where(eq(requests.id, id))
  }

  async updateLastSearched(id: string): Promise<void> {
    await db
      .update(requests)
      .set({ lastSearchedAt: new Date() })
      .where(eq(requests.id, id))
  }

  async updateDownloadInfo(id: string, nzoId: string, releaseName: string): Promise<void> {
    await db.update(requests).set({ nzoId, releaseName }).where(eq(requests.id, id))
  }

  async getApprovedForSearch(): Promise<RequestWithUser[]> {
    return this.getAll({ status: ['approved', 'searching'] })
  }

  async getDownloading(): Promise<RequestWithUser[]> {
    return this.getAll({ status: 'downloading' })
  }

  async getTvForMonitoring(): Promise<RequestWithUser[]> {
    const all = await this.getAll({ type: 'tv', status: 'completed' })
    return all.filter((r) => r.season !== null)
  }

  // Episode management methods

  async createEpisodesForSeason(requestId: string, episodeList: CreateEpisodeData[]): Promise<Episode[]> {
    const now = new Date()
    const created: Episode[] = []

    for (const ep of episodeList) {
      const id = generateId()
      await db.insert(episodes).values({
        id,
        requestId,
        seasonNumber: ep.seasonNumber,
        episodeNumber: ep.episodeNumber,
        title: ep.title ?? null,
        airDate: ep.airDate ?? null,
        status: 'pending',
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
        nzoId: null,
        releaseName: null,
        createdAt: now,
        updatedAt: now,
      })
    }

    // Update request's totalEpisodes count
    await this.updateRequestEpisodeCounts(requestId)

    return created
  }

  async getEpisodes(requestId: string): Promise<Episode[]> {
    const results = await db.select().from(episodes).where(eq(episodes.requestId, requestId))

    return results.map((ep) => ({
      id: ep.id,
      requestId: ep.requestId,
      seasonNumber: ep.seasonNumber,
      episodeNumber: ep.episodeNumber,
      title: ep.title,
      airDate: ep.airDate,
      status: ep.status as EpisodeStatus,
      nzoId: ep.nzoId,
      releaseName: ep.releaseName,
      createdAt: ep.createdAt,
      updatedAt: ep.updatedAt,
    }))
  }

  async getEpisodeById(id: string): Promise<Episode | null> {
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
      nzoId: ep.nzoId,
      releaseName: ep.releaseName,
      createdAt: ep.createdAt,
      updatedAt: ep.updatedAt,
    }
  }

  async updateEpisodeStatus(
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

    // Get episode to find request ID and update counts
    const ep = await this.getEpisodeById(episodeId)
    if (ep) {
      await this.updateRequestEpisodeCounts(ep.requestId)
    }
  }

  async updateAllEpisodesForSeason(
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

  async getEpisodesReadyToSearch(): Promise<EpisodeWithRequest[]> {
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
          // Air date is today or earlier (or null for unknown dates)
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
      nzoId: row.episode.nzoId,
      releaseName: row.episode.releaseName,
      createdAt: row.episode.createdAt,
      updatedAt: row.episode.updatedAt,
      request: row.request,
    }))
  }

  async getPendingEpisodesByRequest(requestId: string): Promise<Episode[]> {
    const allEps = await this.getEpisodes(requestId)
    return allEps.filter((ep) => ep.status === 'pending')
  }

  async updateRequestEpisodeCounts(requestId: string): Promise<void> {
    const allEps = await this.getEpisodes(requestId)
    const totalEpisodes = allEps.length
    const downloadedEpisodes = allEps.filter((ep) => ep.status === 'completed').length

    await db.update(requests).set({ totalEpisodes, downloadedEpisodes }).where(eq(requests.id, requestId))
  }

  async updateRequestSeriesStatus(
    requestId: string,
    seriesStatus: 'returning' | 'ended' | 'upcoming' | 'unknown'
  ): Promise<void> {
    await db.update(requests).set({ seriesStatus }).where(eq(requests.id, requestId))
  }

  async addEpisode(requestId: string, episode: CreateEpisodeData): Promise<Episode> {
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
      nzoId: null,
      releaseName: null,
      createdAt: now,
      updatedAt: now,
    }
  }

  async getTvRequestsWithPendingEpisodes(): Promise<RequestWithUser[]> {
    const tvRequests = await this.getAll({ type: 'tv' })
    const result: RequestWithUser[] = []

    for (const request of tvRequests) {
      if (request.season === null) continue
      const pendingEps = await this.getPendingEpisodesByRequest(request.id)
      if (pendingEps.length > 0) {
        result.push(request)
      }
    }

    return result
  }

  async getReturningSeriesRequests(): Promise<RequestWithUser[]> {
    const tvRequests = await this.getAll({ type: 'tv' })
    return tvRequests.filter((r) => r.seriesStatus === 'returning' || r.seriesStatus === 'upcoming')
  }
}

export const requestService = new RequestService()
