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
  jellyfin: jellyfinSettingsSchema.default({ url: 'http://localhost:8096', syncAdminFromProvider: true }),
  mock: mockAuthSettingsSchema.default({ enabled: false }),
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
  relay: relaySettingsSchema.default({ url: 'https://metadata-relay.dorninger.co' }),
  tmdb: tmdbSettingsSchema.default({ apiKey: '' }),
  tvdb: tvdbSettingsSchema.default({ apiKey: '' }),
})

// UI settings
export const uiSettingsSchema = z.object({
  theme: z.enum(['dark', 'light', 'system']).default('system'),
  language: z.string().default('en'),
  defaultLibraryView: z.enum(['grid', 'list', 'table']).default('grid'),
})

// Request mode permission options
export const requestModePermissionSchema = z.enum([
  'simple_only',
  'detailed_only',
  'user_choice',
])

// Request settings
export const requestSettingsSchema = z.object({
  memberRequestMode: requestModePermissionSchema.default('simple_only'),
  adminRequestMode: requestModePermissionSchema.default('detailed_only'),
  defaultSimpleQuality: z.enum(['2160p', '1080p', '720p', 'any']).default('1080p'),
})

// Size filter settings (based on TRaSH Guides recommendations)
export const sizeFilterSettingsSchema = z.object({
  enabled: z.boolean().default(true),
  minMbPerMin720p: z.number().default(12.5),
  minMbPerMin1080p: z.number().default(12.5),
  minMbPerMin2160p: z.number().default(34.5),
})

// Download settings
export const downloadSettingsSchema = z.object({
  nzbBackupPath: z.string().default(''),
})

// Metadata store settings
export const metadataStoreSettingsSchema = z.object({
  enabled: z.boolean().default(true),
  itemTtlDays: z.number().min(1).max(365).default(30),
  listTtlHours: z.number().min(1).max(168).default(4),
  indexerTrendingTtlMinutes: z.number().min(15).max(1440).default(60),
})

// Downloader categories schema
export const downloaderCategoriesSchema = z.object({
  movies: z.string().optional(),
  tv: z.string().optional(),
  books: z.string().optional(),
  audiobooks: z.string().optional(),
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

// External link providers
export const externalLinkSchema = z.object({
  id: z.string(),
  name: z.string().min(1),
  urlTemplate: z.string().min(1),
  mediaTypes: z.array(z.enum(['movie', 'tv', 'book'])).min(1),
  enabled: z.boolean().default(true),
})

export const defaultExternalLinks: ExternalLink[] = [
  {
    id: 'ereolen',
    name: 'eReolen',
    urlTemplate: 'https://bibliotek.kk.dk/search?q={title}+{author}',
    mediaTypes: ['book'],
    enabled: false,
  },
  {
    id: 'openlibrary',
    name: 'Open Library',
    urlTemplate: 'https://openlibrary.org/works/{openLibraryId}',
    mediaTypes: ['book'],
    enabled: true,
  },
  {
    id: 'goodreads',
    name: 'Goodreads',
    urlTemplate: 'https://www.goodreads.com/search?q={title}',
    mediaTypes: ['book'],
    enabled: true,
  },
  {
    id: 'tmdb-movie',
    name: 'TMDB',
    urlTemplate: 'https://www.themoviedb.org/movie/{tmdbId}',
    mediaTypes: ['movie'],
    enabled: true,
  },
  {
    id: 'tmdb-tv',
    name: 'TMDB',
    urlTemplate: 'https://www.themoviedb.org/tv/{tmdbId}',
    mediaTypes: ['tv'],
    enabled: true,
  },
  {
    id: 'imdb',
    name: 'IMDb',
    urlTemplate: 'https://www.imdb.com/title/{imdbId}',
    mediaTypes: ['movie', 'tv'],
    enabled: true,
  },
]

export const externalLinksSettingsSchema = z.object({
  links: z.array(externalLinkSchema),
})

// Complete settings schema
export const settingsSchema = z.object({
  server: serverSettingsSchema.default({ port: 5001, corsOrigin: 'http://localhost:5173' }),
  database: databaseSettingsSchema.default({ url: './data/jaeger.db' }),
  security: securitySettingsSchema,
  auth: authSettingsSchema.default({
    provider: 'jellyfin',
    jellyfin: { url: 'http://localhost:8096', syncAdminFromProvider: true },
    mock: { enabled: false },
  }),
  media: mediaSettingsSchema.default({ path: '/media', mockFilesystem: false }),
  metadata: metadataSettingsSchema.default({
    providers: ['relay', 'tmdb', 'tvdb'],
    relay: { url: 'https://metadata-relay.dorninger.co' },
    tmdb: { apiKey: '' },
    tvdb: { apiKey: '' },
  }),
  ui: uiSettingsSchema.default({ theme: 'system', language: 'en', defaultLibraryView: 'grid' }),
  requests: requestSettingsSchema.default({
    memberRequestMode: 'simple_only',
    adminRequestMode: 'detailed_only',
    defaultSimpleQuality: '1080p',
  }),
  sizeFilter: sizeFilterSettingsSchema.default({
    enabled: true, minMbPerMin720p: 12.5, minMbPerMin1080p: 12.5, minMbPerMin2160p: 34.5,
  }),
  download: downloadSettingsSchema.default({ nzbBackupPath: '' }),
  metadataStore: metadataStoreSettingsSchema.default({
    enabled: true, itemTtlDays: 30, listTtlHours: 4, indexerTrendingTtlMinutes: 60,
  }),
  externalLinks: externalLinksSettingsSchema.default({ links: defaultExternalLinks }),
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
export type RequestSettings = z.infer<typeof requestSettingsSchema>
export type RequestModePermission = z.infer<typeof requestModePermissionSchema>
export type SizeFilterSettings = z.infer<typeof sizeFilterSettingsSchema>
export type DownloadSettings = z.infer<typeof downloadSettingsSchema>
export type MetadataStoreSettings = z.infer<typeof metadataStoreSettingsSchema>
export type Downloader = z.infer<typeof downloaderSchema>
export type ExternalLink = z.infer<typeof externalLinkSchema>
export type ExternalLinksSettings = z.infer<typeof externalLinksSettingsSchema>
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
  requests: Partial<RequestSettings>
  sizeFilter: Partial<SizeFilterSettings>
  download: Partial<DownloadSettings>
  metadataStore: Partial<MetadataStoreSettings>
  externalLinks: Partial<ExternalLinksSettings>
}>
