import { JellyfinClient, type JellyfinUser } from '@fangst/shared/clients/jellyfin'
import { and, eq } from 'drizzle-orm'
import { db } from '../db'
import { sessions, users } from '../db/schema'
import { configService } from './config'

export type UserRole = 'member' | 'admin'

export type PreferredRequestMode = 'simple' | 'detailed'

export type User = {
  id: string
  providerId: string
  provider: 'jellyfin' | 'plex'
  name: string
  role: UserRole
  preferredRequestMode: PreferredRequestMode
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
  private initialized = false

  private ensureInitialized(): void {
    if (this.initialized) return

    // Initialize Jellyfin client if not in mock mode
    if (!configService.isMockAuth()) {
      const jellyfinUrl = configService.getJellyfinUrl()
      if (jellyfinUrl) {
        this.jellyfinClient = new JellyfinClient(jellyfinUrl)
      }
    }
    this.initialized = true
  }

  async loginWithJellyfin(
    username: string,
    password: string,
    ipAddress: string,
    userAgent?: string
  ): Promise<{ user: User; session: Session }> {
    this.ensureInitialized()

    if (configService.isMockAuth()) {
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

  private async upsertUser(
    jellyfinUser: JellyfinUser,
    provider: 'jellyfin' | 'plex'
  ): Promise<User> {
    const now = new Date()
    const syncAdminFromProvider = configService.getSyncAdminFromProvider()

    const existing = await db
      .select()
      .from(users)
      .where(and(eq(users.providerId, jellyfinUser.id), eq(users.provider, provider)))
      .get()

    if (existing) {
      let role = existing.role as UserRole

      if (syncAdminFromProvider && jellyfinUser.isAdmin && role !== 'admin') {
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
        preferredRequestMode: (existing.preferredRequestMode ?? 'simple') as PreferredRequestMode,
        createdAt: existing.createdAt,
        lastSeenAt: now,
      }
    }

    const isFirstUser = await this.isFirstUser()
    let role: UserRole = 'member'

    if (isFirstUser || (syncAdminFromProvider && jellyfinUser.isAdmin)) {
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
      preferredRequestMode: 'simple' as PreferredRequestMode,
      createdAt: now,
      lastSeenAt: now,
    }
  }

  private async isFirstUser(): Promise<boolean> {
    const count = await db.select().from(users).limit(1).all()
    return count.length === 0
  }

  private async createSession(
    userId: string,
    ipAddress: string,
    userAgent?: string
  ): Promise<Session> {
    const id = generateId()
    const now = new Date()
    const sessionExpiryDays = configService.getSessionExpiryDays()
    const expiresAt = new Date(now.getTime() + sessionExpiryDays * 24 * 60 * 60 * 1000)

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
      preferredRequestMode: (user.preferredRequestMode ?? 'simple') as PreferredRequestMode,
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
    this.ensureInitialized()
    if (configService.isMockAuth()) return ['mock']
    const providers: string[] = []
    if (this.jellyfinClient) providers.push('jellyfin')
    return providers
  }
}

export const authService = new AuthService()
