import { z } from 'zod'

export const resolutionSchema = z.enum(['720p', '1080p', '2160p'])

export const sourceSchema = z.enum(['remux', 'bluray', 'webdl', 'webrip', 'hdtv', 'dvd'])

export const hdrSchema = z.enum(['dv', 'hdr10+', 'hdr10', 'hlg'])

export const audioSchema = z.enum(['atmos', 'truehd', 'dtsx', 'dtshd', 'dts', 'ddp', 'dd', 'flac', 'aac', 'ac3', 'opus'])

export const qualityProfileSchema = z.object({
  id: z.string(),
  name: z.string().min(1),
  // Video
  minResolution: resolutionSchema.default('720p'),
  preferredResolution: resolutionSchema.optional(),
  preferredSources: z.array(sourceSchema).optional(),
  // HDR
  requireHdr: z.boolean().default(false),
  preferredHdr: z.array(hdrSchema).optional(),
  // Audio
  preferredAudio: z.array(audioSchema).optional(),
  // Size filter override
  sizeFilterEnabled: z.boolean().optional(),
  sizeFilterMinMbPerMin: z.number().optional(),
  // Upgrade behavior
  cutoffScore: z.number().min(0).max(100).default(80),
  upgradeAllowed: z.boolean().default(false),
  // Metadata
  isDefault: z.boolean().default(false),
  sortOrder: z.number().default(0),
})

export const createProfileSchema = qualityProfileSchema.omit({ id: true })

export const updateProfileSchema = createProfileSchema.partial()

export type QualityProfile = z.infer<typeof qualityProfileSchema>
export type CreateProfile = z.infer<typeof createProfileSchema>
export type UpdateProfile = z.infer<typeof updateProfileSchema>
