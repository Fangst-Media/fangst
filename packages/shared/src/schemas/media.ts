import { z } from 'zod'

export const mediaTypeSchema = z.enum(['movie', 'series'])

export const qualityProfileSchema = z.object({
  id: z.number(),
  name: z.string(),
})

export const mediaItemSchema = z.object({
  id: z.number(),
  title: z.string(),
  year: z.number().optional(),
  overview: z.string().optional(),
  posterPath: z.string().optional(),
  backdropPath: z.string().optional(),
  mediaType: mediaTypeSchema,
  monitored: z.boolean(),
  qualityProfile: qualityProfileSchema.optional(),
})

export type MediaType = z.infer<typeof mediaTypeSchema>
export type QualityProfile = z.infer<typeof qualityProfileSchema>
export type MediaItem = z.infer<typeof mediaItemSchema>
