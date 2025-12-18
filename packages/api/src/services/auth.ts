import { eq, and } from 'drizzle-orm'
import { db } from '../db'
import { users, sessions } from '../db/schema'
import { JellyfinClient, type JellyfinUser } from '@jaeger/shared/clients/jellyfin'

export type UserRole = 'member' | 'admin'

export type User = {
  id: string
  providerId: string
  provider: 'jellyfin' | 'plex'
  name: string
  role: UserRole
  createdAt: Date
  lastSeenAt: Date | null
}

export type Session = {
  id: string
  userId: string
  expiresAt: Date
}

function generateId(): string {
  return crypto.randomUUID()
}

export class AuthService {
  private jellyfinClient: JellyfinClient | null = null
  private syncAdminFromProvider: boolean
  private sessionExpiryDays: number
  private mockAuth: boolean

  constructor() {
    this.mockAuth = process.env.MOCK_AUTH === 'true'

    const jellyfinUrl = process.env.JELLYFIN_URL
    if (jellyfinUrl && !this.mockAuth) {
      this.jellyfinClient = new JellyfinClient(jellyfinUrl)
    }

    this.syncAdminFromProvider = process.env.SYNC_ADMIN_FROM_PROVIDER !== 'false'
    this.sessionExpiryDays = Number.parseInt(process.env.SESSION_EXPIRY_DAYS || '30', 10)
  }

  async loginWithJellyfin(
    username: string,
    password: string,
    ipAddress: string,
    userAgent?: string
  ): Promise<{ user: User; session: Session }> {
    if (this.mockAuth) {
      return this.loginWithMock(username, ipAddress, userAgent)
    }

    if (!this.jellyfinClient) {
      throw new Error('Jellyfin is not configured')
    }

    const jellyfinUser = await this.jellyfinClient.authenticate(username, password)
    const user = await this.upsertUser(jellyfinUser, 'jellyfin')
    const session = await this.createSession(user.id, ipAddress, userAgent)

    return { user, session }
  }

  private async loginWithMock(
    username: string,
    ipAddress: string,
    userAgent?: string
  ): Promise<{ user: User; session: Session }> {
    const isAdmin = username.toLowerCase() === 'admin'
    const mockUser: JellyfinUser = {
      id: `mock-${username}`,
      name: username,
      isAdmin,
      accessToken: 'mock-token',
    }

    const user = await this.upsertUser(mockUser, 'jellyfin')
    const session = await this.createSession(user.id, ipAddress, userAgent)

    return { user, session }
  }

  private async upsertUser(jellyfinUser: JellyfinUser, provider: 'jellyfin' | 'plex'): Promise<User> {
    const now = new Date()

    const existing = await db
      .select()
      .from(users)
      .where(and(eq(users.providerId, jellyfinUser.id), eq(users.provider, provider)))
      .get()

    if (existing) {
      let role = existing.role as UserRole

      if (this.syncAdminFromProvider && jellyfinUser.isAdmin && role !== 'admin') {
        role = 'admin'
      }

      await db
        .update(users)
        .set({
          name: jellyfinUser.name,
          role,
          lastSeenAt: now,
        })
        .where(eq(users.id, existing.id))

      return {
        id: existing.id,
        providerId: existing.providerId,
        provider: existing.provider as 'jellyfin' | 'plex',
        name: jellyfinUser.name,
        role,
        createdAt: existing.createdAt,
        lastSeenAt: now,
      }
    }

    const isFirstUser = await this.isFirstUser()
    let role: UserRole = 'member'

    if (isFirstUser || (this.syncAdminFromProvider && jellyfinUser.isAdmin)) {
      role = 'admin'
    }

    const id = generateId()

    await db.insert(users).values({
      id,
      providerId: jellyfinUser.id,
      provider,
      name: jellyfinUser.name,
      role,
      createdAt: now,
      lastSeenAt: now,
    })

    return {
      id,
      providerId: jellyfinUser.id,
      provider,
      name: jellyfinUser.name,
      role,
      createdAt: now,
      lastSeenAt: now,
    }
  }

  private async isFirstUser(): Promise<boolean> {
    const count = await db.select().from(users).limit(1).all()
    return count.length === 0
  }

  private async createSession(userId: string, ipAddress: string, userAgent?: string): Promise<Session> {
    const id = generateId()
    const now = new Date()
    const expiresAt = new Date(now.getTime() + this.sessionExpiryDays * 24 * 60 * 60 * 1000)

    await db.insert(sessions).values({
      id,
      userId,
      ipAddress,
      userAgent: userAgent || null,
      expiresAt,
      createdAt: now,
    })

    return { id, userId, expiresAt }
  }

  async validateSession(sessionId: string): Promise<User | null> {
    const session = await db.select().from(sessions).where(eq(sessions.id, sessionId)).get()

    if (!session) {
      return null
    }

    if (session.expiresAt < new Date()) {
      await this.deleteSession(sessionId)
      return null
    }

    const user = await db.select().from(users).where(eq(users.id, session.userId)).get()

    if (!user) {
      return null
    }

    await db.update(users).set({ lastSeenAt: new Date() }).where(eq(users.id, user.id))

    return {
      id: user.id,
      providerId: user.providerId,
      provider: user.provider as 'jellyfin' | 'plex',
      name: user.name,
      role: user.role as UserRole,
      createdAt: user.createdAt,
      lastSeenAt: new Date(),
    }
  }

  async deleteSession(sessionId: string): Promise<void> {
    await db.delete(sessions).where(eq(sessions.id, sessionId))
  }

  async deleteUserSessions(userId: string): Promise<void> {
    await db.delete(sessions).where(eq(sessions.userId, userId))
  }

  getEnabledProviders(): string[] {
    if (this.mockAuth) return ['mock']
    const providers: string[] = []
    if (this.jellyfinClient) providers.push('jellyfin')
    return providers
  }
}

export const authService = new AuthService()
