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
    })
    .optional(),
})

export type Indexer = z.infer<typeof indexerSchema>

export const createIndexerSchema = indexerSchema.omit({ id: true })
export type CreateIndexer = z.infer<typeof createIndexerSchema>

export const updateIndexerSchema = indexerSchema.partial().omit({ id: true })
export type UpdateIndexer = z.infer<typeof updateIndexerSchema>

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
})

export type SearchResult = z.infer<typeof searchResultSchema>

export const searchRequestSchema = z.object({
  query: z.string().optional(),
  imdbId: z.string().optional(),
  tvdbId: z.number().optional(),
  season: z.number().optional(),
  episode: z.number().optional(),
  type: z.enum(['movie', 'tv', 'general']).default('general'),
  categories: z.array(z.number()).optional(),
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
  }
}
