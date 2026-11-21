import type { MetadataRelayResult, TvSeriesDetails } from '@jaeger/shared/clients/metadata-relay'
import { eq } from 'drizzle-orm'
import { db } from '../db'
import { bookMetadata, metadataLists, movieMetadata, tvMetadata } from '../db/schema'
import { logger } from '../lib/logger'
import { configService } from './config'
import type { BookDetails, MetadataService, MovieDetails } from './metadata'

type StoreConfig = {
  enabled: boolean
  itemTtlMs: number
  listTtlMs: number
}

function getConfig(): StoreConfig {
  const settings = configService.get().metadataStore
  return {
    enabled: settings.enabled,
    itemTtlMs: settings.itemTtlDays * 24 * 60 * 60 * 1000,
    listTtlMs: settings.listTtlHours * 60 * 60 * 1000,
  }
}

function isStale(lastRefreshed: Date, ttlMs: number): boolean {
  return Date.now() - lastRefreshed.getTime() > ttlMs
}

// Prevents concurrent fetches for the same key
const inFlight = new Map<string, Promise<unknown>>()

function dedup<T>(key: string, fn: () => Promise<T>): Promise<T> {
  const existing = inFlight.get(key) as Promise<T> | undefined
  if (existing) return existing

  const promise = fn().finally(() => inFlight.delete(key))
  inFlight.set(key, promise)
  return promise
}

export class MetadataStore {
  constructor(private upstream: MetadataService) {}

  // --- Movies ---

  async getMovieDetails(tmdbId: string): Promise<MovieDetails | null> {
    const config = getConfig()
    if (!config.enabled) return this.upstream.getMovieDetails(tmdbId)

    const cached = await db
      .select()
      .from(movieMetadata)
      .where(eq(movieMetadata.id, tmdbId))
      .limit(1)
    if (cached.length > 0) {
      const row = cached[0]!
      if (!isStale(row.lastRefreshedAt, config.itemTtlMs)) {
        return this.rowToMovieDetails(row)
      }
      // Stale — return stale data and refresh in background
      this.refreshMovieInBackground(tmdbId)
      return this.rowToMovieDetails(row)
    }

    return dedup(`movie:${tmdbId}`, async () => {
      const details = await this.upstream.getMovieDetails(tmdbId)
      if (details) await this.storeMovie(details)
      return details
    })
  }

  // --- TV ---

  async getTvDetails(tmdbId: string): Promise<TvSeriesDetails | null> {
    const config = getConfig()
    if (!config.enabled) return this.upstream.getTvDetails(tmdbId)

    const cached = await db.select().from(tvMetadata).where(eq(tvMetadata.id, tmdbId)).limit(1)
    if (cached.length > 0) {
      const row = cached[0]!
      if (!isStale(row.lastRefreshedAt, config.itemTtlMs)) {
        return this.rowToTvDetails(row)
      }
      this.refreshTvInBackground(tmdbId)
      return this.rowToTvDetails(row)
    }

    return dedup(`tv:${tmdbId}`, async () => {
      const details = await this.upstream.getTvDetails(tmdbId)
      if (details) await this.storeTv(details)
      return details
    })
  }

  // --- Books ---

  async getBookDetails(workId: string): Promise<BookDetails | null> {
    const config = getConfig()
    if (!config.enabled) return this.upstream.getBookDetails(workId)

    const cached = await db.select().from(bookMetadata).where(eq(bookMetadata.id, workId)).limit(1)
    if (cached.length > 0) {
      const row = cached[0]!
      if (!isStale(row.lastRefreshedAt, config.itemTtlMs)) {
        return this.rowToBookDetails(row)
      }
      this.refreshBookInBackground(workId)
      return this.rowToBookDetails(row)
    }

    return dedup(`book:${workId}`, async () => {
      const details = await this.upstream.getBookDetails(workId)
      if (details) await this.storeBook(details)
      return details
    })
  }

  // --- Lists (trending, popular, etc.) ---

  async getList(
    listId: string,
    fetchFn: () => Promise<MetadataRelayResult[] | null>
  ): Promise<MetadataRelayResult[] | null> {
    const config = getConfig()
    if (!config.enabled) return fetchFn()

    const cached = await db
      .select()
      .from(metadataLists)
      .where(eq(metadataLists.id, listId))
      .limit(1)
    if (cached.length > 0) {
      const row = cached[0]!
      const results = JSON.parse(row.resultsJson) as MetadataRelayResult[]
      if (!isStale(row.lastRefreshedAt, config.listTtlMs)) {
        return results
      }
      // Stale — return and refresh in background
      this.refreshListInBackground(listId, fetchFn)
      return results
    }

    return dedup(`list:${listId}`, async () => {
      const results = await fetchFn()
      if (results) {
        await this.storeList(listId, results)
        // Store individual items from list results
        this.storeListItems(results)
      }
      return results
    })
  }

  // Convenience wrappers for all list types
  async getTrendingMovies(): Promise<MetadataRelayResult[] | null> {
    return this.getList('trending-movies', () => this.upstream.getTrendingMovies())
  }

  async getTrendingTv(): Promise<MetadataRelayResult[] | null> {
    return this.getList('trending-tv', () => this.upstream.getTrendingTv())
  }

  async getPopularMovies(): Promise<MetadataRelayResult[] | null> {
    return this.getList('popular-movies', () => this.upstream.getPopularMovies())
  }

  async getPopularTv(): Promise<MetadataRelayResult[] | null> {
    return this.getList('popular-tv', () => this.upstream.getPopularTv())
  }

  async getUpcomingMovies(): Promise<MetadataRelayResult[] | null> {
    return this.getList('upcoming-movies', () => this.upstream.getUpcomingMovies())
  }

  async getNowPlayingMovies(): Promise<MetadataRelayResult[] | null> {
    return this.getList('now-playing-movies', () => this.upstream.getNowPlayingMovies())
  }

  async getTopRatedMovies(): Promise<MetadataRelayResult[] | null> {
    return this.getList('top-rated-movies', () => this.upstream.getTopRatedMovies())
  }

  async getTopRatedTv(): Promise<MetadataRelayResult[] | null> {
    return this.getList('top-rated-tv', () => this.upstream.getTopRatedTv())
  }

  async getOnAirTv(): Promise<MetadataRelayResult[] | null> {
    return this.getList('on-air-tv', () => this.upstream.getOnAirTv())
  }

  async getTrendingBooks(): Promise<MetadataRelayResult[] | null> {
    return this.getList('trending-books', () => this.upstream.getTrendingBooks())
  }

  async getPopularBooks(): Promise<MetadataRelayResult[] | null> {
    return this.getList('popular-books', () => this.upstream.getPopularBooks())
  }

  async getClassicBooks(): Promise<MetadataRelayResult[] | null> {
    return this.getList('classic-books', () => this.upstream.getClassicBooks())
  }

  // Search always passes through — no caching
  async searchMovie(query: string, year?: number) {
    return this.upstream.searchMovie(query, year)
  }

  async searchTv(query: string, year?: number) {
    return this.upstream.searchTv(query, year)
  }

  async searchBooks(query: string) {
    return this.upstream.searchBooks(query)
  }

  // Pass-through for methods that don't need caching
  async getSeasonDetails(tmdbId: string | number, seasonNumber: number) {
    return this.upstream.getSeasonDetails(tmdbId, seasonNumber)
  }

  async getCollectionDetails(collectionId: number) {
    return this.upstream.getCollectionDetails(collectionId)
  }

  // --- Storage ---

  private async storeMovie(details: MovieDetails): Promise<void> {
    const now = new Date()
    await db
      .insert(movieMetadata)
      .values({
        id: details.id,
        provider: 'tmdb',
        title: details.title,
        year: details.year ?? null,
        overview: details.overview ?? null,
        posterPath: details.poster ?? null,
        backdropPath: details.backdrop ?? null,
        rating: details.rating ?? null,
        runtime: details.runtime ?? null,
        imdbId: details.imdbId ?? null,
        genres: null,
        collectionJson: details.collection ? JSON.stringify(details.collection) : null,
        releaseDate: null,
        lastRefreshedAt: now,
        createdAt: now,
      })
      .onConflictDoUpdate({
        target: movieMetadata.id,
        set: {
          title: details.title,
          year: details.year ?? null,
          overview: details.overview ?? null,
          posterPath: details.poster ?? null,
          backdropPath: details.backdrop ?? null,
          rating: details.rating ?? null,
          runtime: details.runtime ?? null,
          imdbId: details.imdbId ?? null,
          collectionJson: details.collection ? JSON.stringify(details.collection) : null,
          lastRefreshedAt: now,
        },
      })
  }

  private async storeTv(details: TvSeriesDetails): Promise<void> {
    const now = new Date()
    await db
      .insert(tvMetadata)
      .values({
        id: details.id,
        provider: 'tmdb',
        title: details.title,
        year: details.year ?? null,
        overview: details.overview ?? null,
        posterPath: details.poster ?? null,
        backdropPath: details.backdrop ?? null,
        rating: details.rating ?? null,
        status: details.status ?? null,
        genres: null,
        numberOfSeasons: details.numberOfSeasons ?? null,
        seasonsJson: details.seasons ? JSON.stringify(details.seasons) : null,
        lastRefreshedAt: now,
        createdAt: now,
      })
      .onConflictDoUpdate({
        target: tvMetadata.id,
        set: {
          title: details.title,
          year: details.year ?? null,
          overview: details.overview ?? null,
          posterPath: details.poster ?? null,
          backdropPath: details.backdrop ?? null,
          rating: details.rating ?? null,
          status: details.status ?? null,
          numberOfSeasons: details.numberOfSeasons ?? null,
          seasonsJson: details.seasons ? JSON.stringify(details.seasons) : null,
          lastRefreshedAt: now,
        },
      })
  }

  private async storeBook(details: BookDetails): Promise<void> {
    const now = new Date()
    await db
      .insert(bookMetadata)
      .values({
        id: details.id,
        provider: 'openlibrary',
        title: details.title,
        author: details.author ?? null,
        authorId: details.authorId ?? null,
        year: details.year ?? null,
        overview: details.overview ?? null,
        coverUrl: details.cover ?? null,
        subjects: details.subjects ? JSON.stringify(details.subjects) : null,
        isbn: details.isbn ? JSON.stringify(details.isbn) : null,
        pageCount: details.numberOfPages ?? null,
        lastRefreshedAt: now,
        createdAt: now,
      })
      .onConflictDoUpdate({
        target: bookMetadata.id,
        set: {
          title: details.title,
          author: details.author ?? null,
          authorId: details.authorId ?? null,
          year: details.year ?? null,
          overview: details.overview ?? null,
          coverUrl: details.cover ?? null,
          subjects: details.subjects ? JSON.stringify(details.subjects) : null,
          isbn: details.isbn ? JSON.stringify(details.isbn) : null,
          pageCount: details.numberOfPages ?? null,
          lastRefreshedAt: now,
        },
      })
  }

  private async storeList(listId: string, results: MetadataRelayResult[]): Promise<void> {
    const now = new Date()
    const [mediaType, ...rest] = listId.split('-')
    await db
      .insert(metadataLists)
      .values({
        id: listId,
        listType: rest.join('-') || listId,
        mediaType: mediaType ?? 'unknown',
        provider: null,
        resultsJson: JSON.stringify(results),
        lastRefreshedAt: now,
        createdAt: now,
      })
      .onConflictDoUpdate({
        target: metadataLists.id,
        set: {
          resultsJson: JSON.stringify(results),
          lastRefreshedAt: now,
        },
      })
  }

  // Store individual items from list results to build up local metadata
  private storeListItems(results: MetadataRelayResult[]): void {
    const now = new Date()
    for (const item of results) {
      if (item.type === 'movie') {
        db.insert(movieMetadata)
          .values({
            id: item.id,
            provider: 'tmdb',
            title: item.title,
            year: item.year ?? null,
            overview: item.overview ?? null,
            posterPath: item.poster ?? null,
            backdropPath: item.backdrop ?? null,
            rating: item.rating ?? null,
            runtime: null,
            imdbId: null,
            genres: null,
            collectionJson: null,
            releaseDate: item.releaseDate ?? null,
            lastRefreshedAt: now,
            createdAt: now,
          })
          .onConflictDoNothing()
          .catch(() => {})
      } else if (item.type === 'tv') {
        db.insert(tvMetadata)
          .values({
            id: item.id,
            provider: 'tmdb',
            title: item.title,
            year: item.year ?? null,
            overview: item.overview ?? null,
            posterPath: item.poster ?? null,
            backdropPath: item.backdrop ?? null,
            rating: item.rating ?? null,
            status: null,
            genres: null,
            numberOfSeasons: null,
            seasonsJson: null,
            lastRefreshedAt: now,
            createdAt: now,
          })
          .onConflictDoNothing()
          .catch(() => {})
      } else if (item.type === 'book') {
        db.insert(bookMetadata)
          .values({
            id: item.id,
            provider: 'openlibrary',
            title: item.title,
            author: item.author ?? null,
            authorId: null,
            year: item.year ?? null,
            overview: item.overview ?? null,
            coverUrl: item.poster ?? null,
            subjects: null,
            isbn: null,
            pageCount: null,
            lastRefreshedAt: now,
            createdAt: now,
          })
          .onConflictDoNothing()
          .catch(() => {})
      }
    }
  }

  // --- Row → Type mappers ---

  private rowToMovieDetails(row: typeof movieMetadata.$inferSelect): MovieDetails {
    return {
      id: row.id,
      title: row.title,
      year: row.year ?? undefined,
      overview: row.overview ?? undefined,
      poster: row.posterPath ?? undefined,
      backdrop: row.backdropPath ?? undefined,
      rating: row.rating ?? undefined,
      runtime: row.runtime ?? undefined,
      imdbId: row.imdbId ?? undefined,
      collection: row.collectionJson ? JSON.parse(row.collectionJson) : undefined,
    }
  }

  private rowToTvDetails(row: typeof tvMetadata.$inferSelect): TvSeriesDetails {
    return {
      id: row.id,
      title: row.title,
      year: row.year ?? undefined,
      overview: row.overview ?? undefined,
      poster: row.posterPath ?? undefined,
      backdrop: row.backdropPath ?? undefined,
      rating: row.rating ?? undefined,
      status: row.status ?? 'unknown',
      numberOfSeasons: row.numberOfSeasons ?? 0,
      numberOfEpisodes: 0,
      seasons: row.seasonsJson ? JSON.parse(row.seasonsJson) : [],
    }
  }

  private rowToBookDetails(row: typeof bookMetadata.$inferSelect): BookDetails {
    return {
      id: row.id,
      title: row.title,
      author: row.author ?? undefined,
      authorId: row.authorId ?? undefined,
      year: row.year ?? undefined,
      overview: row.overview ?? undefined,
      cover: row.coverUrl ?? undefined,
      subjects: row.subjects ? JSON.parse(row.subjects) : undefined,
      isbn: row.isbn ? JSON.parse(row.isbn) : undefined,
      numberOfPages: row.pageCount ?? undefined,
    }
  }

  // --- Background refresh ---

  private refreshMovieInBackground(tmdbId: string): void {
    this.upstream
      .getMovieDetails(tmdbId)
      .then((details) => {
        if (details) this.storeMovie(details)
      })
      .catch((err) => logger.warn({ tmdbId, error: err }, 'Background movie refresh failed'))
  }

  private refreshTvInBackground(tmdbId: string): void {
    this.upstream
      .getTvDetails(tmdbId)
      .then((details) => {
        if (details) this.storeTv(details)
      })
      .catch((err) => logger.warn({ tmdbId, error: err }, 'Background TV refresh failed'))
  }

  private refreshBookInBackground(workId: string): void {
    this.upstream
      .getBookDetails(workId)
      .then((details) => {
        if (details) this.storeBook(details)
      })
      .catch((err) => logger.warn({ workId, error: err }, 'Background book refresh failed'))
  }

  private refreshListInBackground(
    listId: string,
    fetchFn: () => Promise<MetadataRelayResult[] | null>
  ): void {
    fetchFn()
      .then((results) => {
        if (results) {
          this.storeList(listId, results)
          this.storeListItems(results)
        }
      })
      .catch((err) => logger.warn({ listId, error: err }, 'Background list refresh failed'))
  }

  // --- Cache management ---

  async purgeExpired(): Promise<void> {
    const config = getConfig()
    const itemCutoff = new Date(Date.now() - config.itemTtlMs * 2)
    const listCutoff = new Date(Date.now() - config.listTtlMs * 2)

    await db.delete(movieMetadata).where(eq(movieMetadata.lastRefreshedAt, itemCutoff))
    await db.delete(tvMetadata).where(eq(tvMetadata.lastRefreshedAt, itemCutoff))
    await db.delete(bookMetadata).where(eq(bookMetadata.lastRefreshedAt, itemCutoff))
    await db.delete(metadataLists).where(eq(metadataLists.lastRefreshedAt, listCutoff))
    logger.info('Purged expired metadata entries')
  }
}
