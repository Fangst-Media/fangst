import { z } from 'zod'

// --- Base result shape used in lanes/lists ---

export const metadataResultSchema = z.object({
  id: z.string(),
  title: z.string(),
  year: z.number().optional(),
  releaseDate: z.string().optional(),
  overview: z.string().optional(),
  poster: z.string().optional(),
  backdrop: z.string().optional(),
  rating: z.number().optional(),
  type: z.enum(['movie', 'tv', 'book']),
  author: z.string().optional(),
})

export type MetadataResult = z.infer<typeof metadataResultSchema>

// --- Lane/List response ---

export const laneResponseSchema = z.object({
  results: z.array(metadataResultSchema),
  unsupported: z.boolean().optional(),
})

export type LaneResponse = z.infer<typeof laneResponseSchema>

// --- Movie details ---

export const movieCollectionSchema = z.object({
  id: z.number(),
  name: z.string(),
  posterPath: z.string().optional(),
  backdropPath: z.string().optional(),
})

export const movieDetailsSchema = z.object({
  id: z.string(),
  title: z.string(),
  year: z.number().optional(),
  overview: z.string().optional(),
  poster: z.string().optional(),
  backdrop: z.string().optional(),
  rating: z.number().optional(),
  runtime: z.number().optional(),
  imdbId: z.string().optional(),
  collection: movieCollectionSchema.optional(),
})

export type MovieDetails = z.infer<typeof movieDetailsSchema>

// --- Collection details ---

export const collectionMovieSchema = z.object({
  id: z.number(),
  title: z.string(),
  overview: z.string().optional(),
  releaseDate: z.string().optional(),
  posterPath: z.string().optional(),
})

export const collectionDetailsSchema = z.object({
  id: z.number(),
  name: z.string(),
  overview: z.string().optional(),
  posterPath: z.string().optional(),
  backdropPath: z.string().optional(),
  movies: z.array(collectionMovieSchema),
})

export type CollectionDetails = z.infer<typeof collectionDetailsSchema>

// --- TV series ---

export const tvSeriesStatusSchema = z.enum(['returning', 'ended', 'upcoming', 'unknown'])

export const tvSeasonSchema = z.object({
  seasonNumber: z.number(),
  name: z.string(),
  episodeCount: z.number(),
  airDate: z.string().optional(),
})

export const tvEpisodeAirSchema = z.object({
  seasonNumber: z.number(),
  episodeNumber: z.number(),
  name: z.string(),
  airDate: z.string().optional(),
})

export const tvSeriesDetailsSchema = z.object({
  id: z.string(),
  title: z.string(),
  year: z.number().optional(),
  overview: z.string().optional(),
  poster: z.string().optional(),
  backdrop: z.string().optional(),
  rating: z.number().optional(),
  status: tvSeriesStatusSchema,
  numberOfSeasons: z.number(),
  numberOfEpisodes: z.number(),
  seasons: z.array(tvSeasonSchema),
  nextEpisodeToAir: tvEpisodeAirSchema.optional(),
  lastEpisodeToAir: tvEpisodeAirSchema.optional(),
})

export type TvSeriesDetails = z.infer<typeof tvSeriesDetailsSchema>

// --- Season details ---

export const seasonEpisodeSchema = z.object({
  episodeNumber: z.number(),
  seasonNumber: z.number(),
  name: z.string(),
  overview: z.string().optional(),
  airDate: z.string().optional(),
})

export const seasonDetailsSchema = z.object({
  seasonNumber: z.number(),
  name: z.string(),
  airDate: z.string().optional(),
  episodes: z.array(seasonEpisodeSchema),
})

export type SeasonDetails = z.infer<typeof seasonDetailsSchema>

// --- Book details ---

export const bookDetailsSchema = z.object({
  id: z.string(),
  title: z.string(),
  author: z.string().optional(),
  authorId: z.string().optional(),
  year: z.number().optional(),
  cover: z.string().optional(),
  overview: z.string().optional(),
  subjects: z.array(z.string()).optional(),
  isbn: z.array(z.string()).optional(),
  numberOfPages: z.number().optional(),
})

export type BookDetails = z.infer<typeof bookDetailsSchema>

// --- Indexer trending ---

export const indexerTrendingItemSchema = z.object({
  title: z.string(),
  grabs: z.number(),
  category: z.string().optional(),
  size: z.number(),
  pubDate: z.string(),
  indexer: z.string(),
  matchedMetadata: metadataResultSchema.optional(),
})

export const indexerTrendingResponseSchema = z.object({
  results: z.array(indexerTrendingItemSchema),
  indexerCount: z.number(),
})

export type IndexerTrendingItem = z.infer<typeof indexerTrendingItemSchema>
export type IndexerTrendingResponse = z.infer<typeof indexerTrendingResponseSchema>

// --- Error response ---

export const errorResponseSchema = z.object({
  error: z.string(),
})

export type ErrorResponse = z.infer<typeof errorResponseSchema>
