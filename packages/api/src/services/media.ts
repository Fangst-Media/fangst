import { eq } from 'drizzle-orm'
import { db } from '../db'
import { metadata, mediaItems } from '../db/schema'
import type { ScannedFile } from './scanner'

export type MediaItemWithMetadata = {
  id: string
  path: string
  filename: string
  parsed: {
    title: string
    year?: number
    season?: number
    episode?: number
    resolution?: string
    source?: string
    videoCodec?: string
    audioCodec?: string
    hdr?: string
    releaseGroup?: string
    proper?: boolean
    repack?: boolean
  }
  metadata?: {
    id: string
    title: string
    year?: number
    overview?: string
    posterPath?: string
    backdropPath?: string
    rating?: number
    provider: string
  }
}

function generateId(): string {
  return crypto.randomUUID()
}

export class MediaService {
  async getAll(): Promise<MediaItemWithMetadata[]> {
    const results = await db
      .select()
      .from(mediaItems)
      .leftJoin(metadata, eq(mediaItems.metadataId, metadata.id))

    return results.map((row) => ({
      id: row.media_items.id,
      path: row.media_items.path,
      filename: row.media_items.filename,
      parsed: {
        title: row.media_items.title,
        year: row.media_items.year ?? undefined,
        season: row.media_items.season ?? undefined,
        episode: row.media_items.episode ?? undefined,
        resolution: row.media_items.resolution ?? undefined,
        source: row.media_items.source ?? undefined,
        videoCodec: row.media_items.videoCodec ?? undefined,
        audioCodec: row.media_items.audioCodec ?? undefined,
        hdr: row.media_items.hdr ?? undefined,
        releaseGroup: row.media_items.releaseGroup ?? undefined,
        proper: row.media_items.proper ?? undefined,
        repack: row.media_items.repack ?? undefined,
      },
      metadata: row.metadata
        ? {
            id: row.metadata.id,
            title: row.metadata.title,
            year: row.metadata.year ?? undefined,
            overview: row.metadata.overview ?? undefined,
            posterPath: row.metadata.posterPath ?? undefined,
            backdropPath: row.metadata.backdropPath ?? undefined,
            rating: row.metadata.rating ?? undefined,
            provider: row.metadata.provider,
          }
        : undefined,
    }))
  }

  async getMovies(): Promise<MediaItemWithMetadata[]> {
    const all = await this.getAll()
    return all.filter((item) => item.parsed.season === undefined)
  }

  async getTvShows(): Promise<MediaItemWithMetadata[]> {
    const all = await this.getAll()
    return all.filter((item) => item.parsed.season !== undefined)
  }

  async syncFromScan(scannedFiles: ScannedFile[]): Promise<{ added: number; updated: number; removed: number }> {
    const now = new Date()
    const scannedPaths = new Set(scannedFiles.map((f) => f.path))

    const existingItems = await db.select().from(mediaItems)
    const existingByPath = new Map(existingItems.map((item) => [item.path, item]))

    let added = 0
    let updated = 0
    let removed = 0

    for (const file of scannedFiles) {
      const existing = existingByPath.get(file.path)

      let metadataId: string | null = null
      if (file.metadata) {
        metadataId = `${file.metadata.provider}:${file.metadata.id}`
        await this.upsertMetadata(metadataId, file.metadata, file.parsed.season !== undefined ? 'tv' : 'movie', now)
      }

      if (existing) {
        await db
          .update(mediaItems)
          .set({
            filename: file.filename,
            title: file.parsed.title,
            year: file.parsed.year ?? null,
            season: file.parsed.season ?? null,
            episode: file.parsed.episode ?? null,
            resolution: file.parsed.resolution ?? null,
            source: file.parsed.source ?? null,
            videoCodec: file.parsed.videoCodec ?? null,
            audioCodec: file.parsed.audioCodec ?? null,
            hdr: file.parsed.hdr ?? null,
            releaseGroup: file.parsed.releaseGroup ?? null,
            proper: file.parsed.proper ?? null,
            repack: file.parsed.repack ?? null,
            metadataId,
            updatedAt: now,
          })
          .where(eq(mediaItems.id, existing.id))
        updated++
      } else {
        await db.insert(mediaItems).values({
          id: generateId(),
          path: file.path,
          filename: file.filename,
          title: file.parsed.title,
          year: file.parsed.year ?? null,
          season: file.parsed.season ?? null,
          episode: file.parsed.episode ?? null,
          resolution: file.parsed.resolution ?? null,
          source: file.parsed.source ?? null,
          videoCodec: file.parsed.videoCodec ?? null,
          audioCodec: file.parsed.audioCodec ?? null,
          hdr: file.parsed.hdr ?? null,
          releaseGroup: file.parsed.releaseGroup ?? null,
          proper: file.parsed.proper ?? null,
          repack: file.parsed.repack ?? null,
          metadataId,
          createdAt: now,
          updatedAt: now,
        })
        added++
      }
    }

    for (const existing of existingItems) {
      if (!scannedPaths.has(existing.path)) {
        await db.delete(mediaItems).where(eq(mediaItems.id, existing.id))
        removed++
      }
    }

    return { added, updated, removed }
  }

  private async upsertMetadata(
    id: string,
    meta: NonNullable<ScannedFile['metadata']>,
    type: 'movie' | 'tv',
    now: Date
  ): Promise<void> {
    const existing = await db.select().from(metadata).where(eq(metadata.id, id)).get()

    if (existing) {
      await db
        .update(metadata)
        .set({
          title: meta.title,
          year: meta.year ?? null,
          overview: meta.overview ?? null,
          posterPath: meta.posterPath ?? null,
          backdropPath: meta.backdropPath ?? null,
          rating: meta.rating ?? null,
          updatedAt: now,
        })
        .where(eq(metadata.id, id))
    } else {
      await db.insert(metadata).values({
        id,
        provider: meta.provider,
        type,
        title: meta.title,
        year: meta.year ?? null,
        overview: meta.overview ?? null,
        posterPath: meta.posterPath ?? null,
        backdropPath: meta.backdropPath ?? null,
        rating: meta.rating ?? null,
        createdAt: now,
        updatedAt: now,
      })
    }
  }

  async clear(): Promise<void> {
    await db.delete(mediaItems)
    await db.delete(metadata)
  }
}

export const mediaService = new MediaService()
