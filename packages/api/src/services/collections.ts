import { eq } from 'drizzle-orm'
import { db } from '../db'
import { collections, mediaItems, metadata } from '../db/schema'
import { configService } from './config'
import { getMetadataService } from './metadata'

export type Collection = {
  id: string
  tmdbCollectionId: number
  name: string
  overview?: string
  posterPath?: string
  backdropPath?: string
  createdAt: Date
  updatedAt: Date
}

export type CollectionOverview = {
  id: string
  tmdbCollectionId: number
  name: string
  posterPath?: string
  backdropPath?: string
  movieCount: number
  downloadedCount: number
}

export type CollectionMovie = {
  tmdbId: number
  title: string
  overview?: string
  releaseDate?: string
  posterPath?: string
  downloaded: boolean
  file?: {
    id: string
    filename: string
    resolution?: string
    size?: number
  }
}

export type CollectionWithMovies = {
  collection: Collection
  movies: CollectionMovie[]
  totalMovies: number
  downloadedMovies: number
}

function generateId(): string {
  return crypto.randomUUID()
}

export class CollectionService {
  async getOrCreateCollection(tmdbCollectionId: number): Promise<Collection> {
    const existing = await db
      .select()
      .from(collections)
      .where(eq(collections.tmdbCollectionId, tmdbCollectionId))
      .limit(1)

    if (existing.length > 0) {
      return {
        id: existing[0]!.id,
        tmdbCollectionId: existing[0]!.tmdbCollectionId,
        name: existing[0]!.name,
        overview: existing[0]!.overview ?? undefined,
        posterPath: existing[0]!.posterPath ?? undefined,
        backdropPath: existing[0]!.backdropPath ?? undefined,
        createdAt: existing[0]!.createdAt,
        updatedAt: existing[0]!.updatedAt,
      }
    }

    const metadataService = getMetadataService({
      relayUrl: configService.getMetadataRelayUrl(),
      tmdbApiKey: configService.getTmdbApiKey(),
    })

    const details = await metadataService.getCollectionDetails(tmdbCollectionId)
    if (!details) {
      throw new Error(`Collection ${tmdbCollectionId} not found`)
    }

    const now = new Date()
    const id = generateId()

    await db.insert(collections).values({
      id,
      tmdbCollectionId,
      name: details.name,
      overview: details.overview,
      posterPath: details.posterPath,
      backdropPath: details.backdropPath,
      createdAt: now,
      updatedAt: now,
    })

    return {
      id,
      tmdbCollectionId,
      name: details.name,
      overview: details.overview,
      posterPath: details.posterPath,
      backdropPath: details.backdropPath,
      createdAt: now,
      updatedAt: now,
    }
  }

  async getCollectionsInLibrary(): Promise<CollectionOverview[]> {
    // Get all metadata with collection IDs
    const metadataWithCollections = await db
      .select({
        tmdbCollectionId: metadata.tmdbCollectionId,
        metadataId: metadata.id,
      })
      .from(metadata)
      .where(eq(metadata.type, 'movie'))

    // Group by collection ID
    const collectionMetadataMap = new Map<number, string[]>()
    for (const item of metadataWithCollections) {
      if (item.tmdbCollectionId) {
        const existing = collectionMetadataMap.get(item.tmdbCollectionId) ?? []
        existing.push(item.metadataId)
        collectionMetadataMap.set(item.tmdbCollectionId, existing)
      }
    }

    if (collectionMetadataMap.size === 0) {
      return []
    }

    // Get collection details from DB or fetch from TMDB
    const results: CollectionOverview[] = []

    for (const [tmdbCollectionId, metadataIds] of collectionMetadataMap) {
      // Try to get collection from DB first
      let collection = await db
        .select()
        .from(collections)
        .where(eq(collections.tmdbCollectionId, tmdbCollectionId))
        .limit(1)

      // If not in DB, fetch from TMDB
      if (collection.length === 0) {
        try {
          await this.getOrCreateCollection(tmdbCollectionId)
          collection = await db
            .select()
            .from(collections)
            .where(eq(collections.tmdbCollectionId, tmdbCollectionId))
            .limit(1)
        } catch {
          continue
        }
      }

      if (collection.length > 0) {
        // Get full collection details to know total movie count
        const metadataService = getMetadataService({
          relayUrl: configService.getMetadataRelayUrl(),
          tmdbApiKey: configService.getTmdbApiKey(),
        })
        const details = await metadataService.getCollectionDetails(tmdbCollectionId)

        results.push({
          id: collection[0]!.id,
          tmdbCollectionId: collection[0]!.tmdbCollectionId,
          name: collection[0]!.name,
          posterPath: collection[0]!.posterPath ?? undefined,
          backdropPath: collection[0]!.backdropPath ?? undefined,
          movieCount: details?.movies.length ?? metadataIds.length,
          downloadedCount: metadataIds.length,
        })
      }
    }

    return results.sort((a, b) => a.name.localeCompare(b.name))
  }

  async getCollectionWithMovies(tmdbCollectionId: number): Promise<CollectionWithMovies | null> {
    const metadataService = getMetadataService({
      relayUrl: configService.getMetadataRelayUrl(),
      tmdbApiKey: configService.getTmdbApiKey(),
    })

    const details = await metadataService.getCollectionDetails(tmdbCollectionId)
    if (!details) {
      return null
    }

    // Ensure collection is in DB
    const collection = await this.getOrCreateCollection(tmdbCollectionId)

    // Get all movies in library that belong to this collection
    const moviesInLibrary = await db
      .select({
        metadataId: metadata.id,
        tmdbId: metadata.id,
        mediaItemId: mediaItems.id,
        filename: mediaItems.filename,
        resolution: mediaItems.resolution,
        size: mediaItems.size,
      })
      .from(metadata)
      .leftJoin(mediaItems, eq(mediaItems.metadataId, metadata.id))
      .where(eq(metadata.tmdbCollectionId, tmdbCollectionId))

    // Create a map of tmdb ID to downloaded file
    const downloadedMap = new Map<
      string,
      { id: string; filename: string; resolution?: string; size?: number }
    >()
    for (const item of moviesInLibrary) {
      if (item.mediaItemId) {
        // Extract TMDB ID from metadata ID (format: "tmdb:12345")
        const tmdbId = item.metadataId.replace(/^[^:]+:/, '')
        downloadedMap.set(tmdbId, {
          id: item.mediaItemId,
          filename: item.filename!,
          resolution: item.resolution ?? undefined,
          size: item.size ?? undefined,
        })
      }
    }

    // Build movie list with download status
    const movies: CollectionMovie[] = details.movies.map((movie) => {
      const file = downloadedMap.get(movie.id.toString())
      return {
        tmdbId: movie.id,
        title: movie.title,
        overview: movie.overview,
        releaseDate: movie.releaseDate,
        posterPath: movie.posterPath,
        downloaded: !!file,
        file,
      }
    })

    // Sort by release date
    movies.sort((a, b) => {
      if (!a.releaseDate) return 1
      if (!b.releaseDate) return -1
      return a.releaseDate.localeCompare(b.releaseDate)
    })

    return {
      collection,
      movies,
      totalMovies: movies.length,
      downloadedMovies: movies.filter((m) => m.downloaded).length,
    }
  }

  async getById(id: string): Promise<Collection | null> {
    const result = await db.select().from(collections).where(eq(collections.id, id)).limit(1)

    if (result.length === 0) {
      return null
    }

    return {
      id: result[0]!.id,
      tmdbCollectionId: result[0]!.tmdbCollectionId,
      name: result[0]!.name,
      overview: result[0]!.overview ?? undefined,
      posterPath: result[0]!.posterPath ?? undefined,
      backdropPath: result[0]!.backdropPath ?? undefined,
      createdAt: result[0]!.createdAt,
      updatedAt: result[0]!.updatedAt,
    }
  }
}

export const collectionService = new CollectionService()
