import { eq, inArray, and, isNull } from 'drizzle-orm'
import { db } from '../db'
import { requests, users, requestUsers } from '../db/schema'

export type RequestStatus =
  | 'pending_approval'
  | 'approved'
  | 'searching'
  | 'downloading'
  | 'completed'
  | 'rejected'

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

  async getApprovedForSearch(): Promise<RequestWithUser[]> {
    return this.getAll({ status: ['approved', 'searching'] })
  }

  async getTvForMonitoring(): Promise<RequestWithUser[]> {
    const all = await this.getAll({ type: 'tv', status: 'completed' })
    return all.filter((r) => r.season !== null)
  }
}

export const requestService = new RequestService()
