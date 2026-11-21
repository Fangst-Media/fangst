/**
 * API types — canonical re-exports from the shared Zod schemas.
 *
 * Run `bun run generate:types` to regenerate the OpenAPI client.
 * The shared schemas are the single source of truth for API shapes.
 */

export type {
  MetadataResult,
  LaneResponse,
  MovieDetails,
  CollectionDetails,
  TvSeriesDetails,
  SeasonDetails,
  BookDetails,
  IndexerTrendingItem,
  IndexerTrendingResponse,
} from '@fangst/shared/schemas/metadata'
