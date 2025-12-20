import { z } from 'zod'

// Server settings
export const serverSettingsSchema = z.object({
  port: z.number().min(1).max(65535).default(5001),
  corsOrigin: z.string().default('http://localhost:5173'),
})

// Database settings
export const databaseSettingsSchema = z.object({
  url: z.string().default('./data/jaeger.db'),
})

// Security settings
export const securitySettingsSchema = z.object({
  apiKey: z.string().min(32),
  sessionExpiryDays: z.number().min(1).max(365).default(30),
})

// Jellyfin auth settings
export const jellyfinSettingsSchema = z.object({
  url: z.string().default('http://localhost:8096'),
  syncAdminFromProvider: z.boolean().default(true),
})

// Mock auth settings
export const mockAuthSettingsSchema = z.object({
  enabled: z.boolean().default(false),
})

// Auth settings
export const authSettingsSchema = z.object({
  provider: z.enum(['jellyfin', 'mock']).default('jellyfin'),
  jellyfin: jellyfinSettingsSchema.default({}),
  mock: mockAuthSettingsSchema.default({}),
})

// Media settings
export const mediaSettingsSchema = z.object({
  path: z.string().default('/media'),
  mockFilesystem: z.boolean().default(false),
})

// Metadata provider settings
export const relaySettingsSchema = z.object({
  url: z.string().default('https://metadata-relay.dorninger.co'),
})

export const tmdbSettingsSchema = z.object({
  apiKey: z.string().default(''),
})

export const tvdbSettingsSchema = z.object({
  apiKey: z.string().default(''),
})

export const metadataSettingsSchema = z.object({
  providers: z.array(z.enum(['relay', 'tmdb', 'tvdb'])).default(['relay', 'tmdb', 'tvdb']),
  relay: relaySettingsSchema.default({}),
  tmdb: tmdbSettingsSchema.default({}),
  tvdb: tvdbSettingsSchema.default({}),
})

// UI settings
export const uiSettingsSchema = z.object({
  theme: z.enum(['dark', 'light', 'system']).default('system'),
  language: z.string().default('en'),
  defaultLibraryView: z.enum(['grid', 'list', 'table']).default('grid'),
})

// Downloader categories schema
export const downloaderCategoriesSchema = z.object({
  movies: z.string().optional(),
  tv: z.string().optional(),
})

// Downloader schema
export const downloaderSchema = z.object({
  id: z.string(),
  name: z.string().min(1),
  type: z.enum(['sabnzbd']),
  url: z.string().url(),
  apiKey: z.string(),
  enabled: z.boolean().default(true),
  categories: downloaderCategoriesSchema.optional(),
})

// Create schema (without id, generated server-side)
export const createDownloaderSchema = downloaderSchema.omit({ id: true })

// Update schema (all fields optional except must have at least one)
export const updateDownloaderSchema = downloaderSchema.partial().omit({ id: true })

// Complete settings schema
export const settingsSchema = z.object({
  server: serverSettingsSchema.default({}),
  database: databaseSettingsSchema.default({}),
  security: securitySettingsSchema,
  auth: authSettingsSchema.default({}),
  media: mediaSettingsSchema.default({}),
  metadata: metadataSettingsSchema.default({}),
  ui: uiSettingsSchema.default({}),
})

// Inferred types
export type Settings = z.infer<typeof settingsSchema>
export type ServerSettings = z.infer<typeof serverSettingsSchema>
export type DatabaseSettings = z.infer<typeof databaseSettingsSchema>
export type SecuritySettings = z.infer<typeof securitySettingsSchema>
export type AuthSettings = z.infer<typeof authSettingsSchema>
export type MediaSettings = z.infer<typeof mediaSettingsSchema>
export type MetadataSettings = z.infer<typeof metadataSettingsSchema>
export type UISettings = z.infer<typeof uiSettingsSchema>
export type Downloader = z.infer<typeof downloaderSchema>
export type DownloaderCategories = z.infer<typeof downloaderCategoriesSchema>
export type CreateDownloader = z.infer<typeof createDownloaderSchema>
export type UpdateDownloader = z.infer<typeof updateDownloaderSchema>

// Locked settings tracking - indicates which settings are controlled by ENV
export type LockedSettings = {
  [K in keyof Settings]?: {
    [P in keyof Settings[K]]?: boolean
  }
}

// API response includes both values and lock status
export type SettingsResponse = {
  settings: Settings
  locked: LockedSettings
}

// Partial settings for PATCH updates
export type SettingsUpdate = Partial<{
  server: Partial<ServerSettings>
  security: Partial<Pick<SecuritySettings, 'sessionExpiryDays'>>
  auth: Partial<AuthSettings>
  media: Partial<MediaSettings>
  metadata: Partial<MetadataSettings>
  ui: Partial<UISettings>
}>
