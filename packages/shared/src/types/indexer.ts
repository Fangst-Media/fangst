import { z } from 'zod'

export const indexerProtocols = ['newznab'] as const
export type IndexerProtocol = (typeof indexerProtocols)[number]

export const indexerSchema = z.object({
  id: z.string(),
  name: z.string().min(1),
  protocol: z.enum(indexerProtocols),
  url: z.string().url(),
  apiKey: z.string().min(1),
  enabled: z.boolean().default(true),
  categories: z
    .object({
      movies: z.array(z.number()).optional(),
      tv: z.array(z.number()).optional(),
      books: z.array(z.number()).optional(),
      audiobooks: z.array(z.number()).optional(),
    })
    .optional(),
  apiPath: z.string().default('/api'),
  priority: z.number().min(1).max(100).default(50),
  queryLimit: z.number().positive().nullable().optional(),
  grabLimit: z.number().positive().nullable().optional(),
})

export type Indexer = z.infer<typeof indexerSchema>

export const createIndexerSchema = indexerSchema.omit({ id: true })
export type CreateIndexer = z.infer<typeof createIndexerSchema>

export const updateIndexerSchema = indexerSchema.partial().omit({ id: true })
export type UpdateIndexer = z.infer<typeof updateIndexerSchema>

export const parsedReleaseInfoSchema = z.object({
  resolution: z.enum(['2160p', '1080p', '720p', '480p']).optional(),
  source: z.enum(['bluray', 'webdl', 'webrip', 'hdtv', 'dvd']).optional(),
  videoCodec: z.enum(['x265', 'x264', 'av1', 'xvid']).optional(),
  audioCodec: z.enum(['atmos', 'truehd', 'dtsx', 'dtshd', 'dts', 'ddp', 'dd', 'flac', 'aac', 'ac3', 'opus']).optional(),
  hdr: z.enum(['dv', 'hdr10+', 'hdr10', 'hlg']).optional(),
  releaseGroup: z.string().optional(),
  releaseGroupTier: z.union([z.literal(1), z.literal(2), z.literal(3), z.null()]).optional(),
  streamingService: z.string().optional(),
  isRemux: z.boolean().optional(),
  proper: z.boolean().optional(),
  repack: z.boolean().optional(),
  qualityScore: z.number().optional(),
})

export type ParsedReleaseInfo = z.infer<typeof parsedReleaseInfoSchema>

export const searchResultSchema = z.object({
  guid: z.string(),
  title: z.string(),
  indexer: z.string(),
  indexerId: z.string(),
  downloadUrl: z.string().url(),
  size: z.number(),
  pubDate: z.string(),
  category: z.string().optional(),
  grabs: z.number().optional(),
  seeders: z.number().optional(),
  peers: z.number().optional(),
  parsed: parsedReleaseInfoSchema.optional(),
})

export type SearchResult = z.infer<typeof searchResultSchema>

export const searchRequestSchema = z.object({
  query: z.string().optional(),
  imdbId: z.string().optional(),
  tvdbId: z.number().optional(),
  season: z.number().optional(),
  episode: z.number().optional(),
  type: z.enum(['movie', 'tv', 'book', 'general']).default('general'),
  categories: z.array(z.number()).optional(),
  runtime: z.number().optional(),
})

export type SearchRequest = z.infer<typeof searchRequestSchema>

export const searchResponseSchema = z.object({
  results: z.array(searchResultSchema),
  errors: z
    .array(
      z.object({
        indexer: z.string(),
        error: z.string(),
      })
    )
    .optional(),
})

export type SearchResponse = z.infer<typeof searchResponseSchema>

export const indexerTestResultSchema = z.object({
  success: z.boolean(),
  version: z.string().optional(),
  error: z.string().optional(),
})

export type IndexerTestResult = z.infer<typeof indexerTestResultSchema>

export interface IndexerRegistryEntry {
  name: string
  protocol: IndexerProtocol
  defaultUrl?: string
  categories?: {
    movies?: number[]
    tv?: number[]
    books?: number[]
    audiobooks?: number[]
  }
  apiPath?: string
}
