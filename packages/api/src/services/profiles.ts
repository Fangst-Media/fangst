import type { CreateProfile, QualityProfile, UpdateProfile } from '@jaeger/shared/types/profiles'
import { eq } from 'drizzle-orm'
import { db } from '../db'
import { qualityProfiles } from '../db/schema'

function generateId(): string {
  return crypto.randomUUID()
}

function rowToProfile(row: typeof qualityProfiles.$inferSelect): QualityProfile {
  return {
    id: row.id,
    name: row.name,
    minResolution: row.minResolution as QualityProfile['minResolution'],
    preferredResolution: row.preferredResolution as QualityProfile['preferredResolution'],
    preferredSources: row.preferredSources ? JSON.parse(row.preferredSources) : undefined,
    requireHdr: row.requireHdr,
    preferredHdr: row.preferredHdr ? JSON.parse(row.preferredHdr) : undefined,
    preferredAudio: row.preferredAudio ? JSON.parse(row.preferredAudio) : undefined,
    sizeFilterEnabled: row.sizeFilterEnabled ?? undefined,
    sizeFilterMinMbPerMin: row.sizeFilterMinMbPerMin ?? undefined,
    cutoffScore: row.cutoffScore,
    upgradeAllowed: row.upgradeAllowed,
    isDefault: row.isDefault,
    sortOrder: row.sortOrder,
  }
}

class ProfileService {
  async getAll(): Promise<QualityProfile[]> {
    const rows = await db.select().from(qualityProfiles).orderBy(qualityProfiles.sortOrder)
    return rows.map(rowToProfile)
  }

  async getById(id: string): Promise<QualityProfile | null> {
    const rows = await db.select().from(qualityProfiles).where(eq(qualityProfiles.id, id)).limit(1)
    return rows[0] ? rowToProfile(rows[0]) : null
  }

  async getDefault(): Promise<QualityProfile | null> {
    const rows = await db
      .select()
      .from(qualityProfiles)
      .where(eq(qualityProfiles.isDefault, true))
      .limit(1)
    return rows[0] ? rowToProfile(rows[0]) : null
  }

  async create(data: CreateProfile): Promise<QualityProfile> {
    const now = new Date()
    const id = generateId()

    if (data.isDefault) {
      await db.update(qualityProfiles).set({ isDefault: false })
    }

    await db.insert(qualityProfiles).values({
      id,
      name: data.name,
      minResolution: data.minResolution ?? '720p',
      preferredResolution: data.preferredResolution ?? null,
      preferredSources: data.preferredSources ? JSON.stringify(data.preferredSources) : null,
      requireHdr: data.requireHdr ?? false,
      preferredHdr: data.preferredHdr ? JSON.stringify(data.preferredHdr) : null,
      preferredAudio: data.preferredAudio ? JSON.stringify(data.preferredAudio) : null,
      sizeFilterEnabled: data.sizeFilterEnabled ?? null,
      sizeFilterMinMbPerMin: data.sizeFilterMinMbPerMin ?? null,
      cutoffScore: data.cutoffScore ?? 80,
      upgradeAllowed: data.upgradeAllowed ?? false,
      isDefault: data.isDefault ?? false,
      sortOrder: data.sortOrder ?? 0,
      createdAt: now,
      updatedAt: now,
    })

    const created = await this.getById(id)
    if (!created) throw new Error('Failed to create profile')
    return created
  }

  async update(id: string, data: UpdateProfile): Promise<QualityProfile> {
    const existing = await this.getById(id)
    if (!existing) throw new Error('Profile not found')

    if (data.isDefault) {
      await db.update(qualityProfiles).set({ isDefault: false })
    }

    const updates: Record<string, unknown> = { updatedAt: new Date() }

    if (data.name !== undefined) updates.name = data.name
    if (data.minResolution !== undefined) updates.minResolution = data.minResolution
    if (data.preferredResolution !== undefined)
      updates.preferredResolution = data.preferredResolution
    if (data.preferredSources !== undefined)
      updates.preferredSources = data.preferredSources
        ? JSON.stringify(data.preferredSources)
        : null
    if (data.requireHdr !== undefined) updates.requireHdr = data.requireHdr
    if (data.preferredHdr !== undefined)
      updates.preferredHdr = data.preferredHdr ? JSON.stringify(data.preferredHdr) : null
    if (data.preferredAudio !== undefined)
      updates.preferredAudio = data.preferredAudio ? JSON.stringify(data.preferredAudio) : null
    if (data.sizeFilterEnabled !== undefined) updates.sizeFilterEnabled = data.sizeFilterEnabled
    if (data.sizeFilterMinMbPerMin !== undefined)
      updates.sizeFilterMinMbPerMin = data.sizeFilterMinMbPerMin
    if (data.cutoffScore !== undefined) updates.cutoffScore = data.cutoffScore
    if (data.upgradeAllowed !== undefined) updates.upgradeAllowed = data.upgradeAllowed
    if (data.isDefault !== undefined) updates.isDefault = data.isDefault
    if (data.sortOrder !== undefined) updates.sortOrder = data.sortOrder

    await db.update(qualityProfiles).set(updates).where(eq(qualityProfiles.id, id))

    const updated = await this.getById(id)
    if (!updated) throw new Error('Failed to update profile')
    return updated
  }

  async delete(id: string): Promise<void> {
    const existing = await this.getById(id)
    if (!existing) throw new Error('Profile not found')
    await db.delete(qualityProfiles).where(eq(qualityProfiles.id, id))
  }

  async seedDefaults(): Promise<void> {
    const existing = await db.select({ id: qualityProfiles.id }).from(qualityProfiles).limit(1)
    if (existing.length > 0) return

    const now = new Date()
    const defaults = [
      {
        id: generateId(),
        name: '4K Remux',
        minResolution: '2160p' as const,
        preferredResolution: '2160p' as const,
        preferredSources: JSON.stringify(['remux', 'bluray']),
        requireHdr: false,
        preferredHdr: JSON.stringify(['dv', 'hdr10+', 'hdr10']),
        preferredAudio: JSON.stringify(['atmos', 'truehd', 'dtshd']),
        sizeFilterEnabled: null,
        sizeFilterMinMbPerMin: null,
        cutoffScore: 90,
        upgradeAllowed: true,
        isDefault: false,
        sortOrder: 0,
        createdAt: now,
        updatedAt: now,
      },
      {
        id: generateId(),
        name: '1080p Quality',
        minResolution: '1080p' as const,
        preferredResolution: '1080p' as const,
        preferredSources: JSON.stringify(['bluray', 'webdl']),
        requireHdr: false,
        preferredHdr: null,
        preferredAudio: JSON.stringify(['truehd', 'dtshd', 'ddp']),
        sizeFilterEnabled: null,
        sizeFilterMinMbPerMin: null,
        cutoffScore: 70,
        upgradeAllowed: true,
        isDefault: true,
        sortOrder: 1,
        createdAt: now,
        updatedAt: now,
      },
      {
        id: generateId(),
        name: 'Any HD',
        minResolution: '720p' as const,
        preferredResolution: '1080p' as const,
        preferredSources: JSON.stringify(['webdl', 'bluray', 'webrip']),
        requireHdr: false,
        preferredHdr: null,
        preferredAudio: null,
        sizeFilterEnabled: null,
        sizeFilterMinMbPerMin: null,
        cutoffScore: 50,
        upgradeAllowed: false,
        isDefault: false,
        sortOrder: 2,
        createdAt: now,
        updatedAt: now,
      },
    ]

    for (const profile of defaults) {
      await db.insert(qualityProfiles).values(profile)
    }
  }
}

export const profileService = new ProfileService()
