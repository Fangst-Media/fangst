import { integer, sqliteTable, text, real } from 'drizzle-orm/sqlite-core'

export const arrInstances = sqliteTable('arr_instances', {
  id: text('id').primaryKey(),
  name: text('name').notNull(),
  type: text('type', { enum: ['sonarr', 'radarr'] }).notNull(),
  url: text('url').notNull(),
  apiKey: text('api_key').notNull(),
  enabled: integer('enabled', { mode: 'boolean' }).notNull().default(true),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
  updatedAt: integer('updated_at', { mode: 'timestamp' }).notNull(),
})

export const users = sqliteTable('users', {
  id: text('id').primaryKey(),
  providerId: text('provider_id').notNull(),
  provider: text('provider', { enum: ['jellyfin', 'plex'] }).notNull(),
  name: text('name').notNull(),
  role: text('role', { enum: ['member', 'admin'] }).notNull().default('member'),
  preferredRequestMode: text('preferred_request_mode', { enum: ['simple', 'detailed'] }).default('simple'),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
  lastSeenAt: integer('last_seen_at', { mode: 'timestamp' }),
})

export const sessions = sqliteTable('sessions', {
  id: text('id').primaryKey(),
  userId: text('user_id').notNull().references(() => users.id),
  ipAddress: text('ip_address').notNull(),
  userAgent: text('user_agent'),
  expiresAt: integer('expires_at', { mode: 'timestamp' }).notNull(),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
})

export const networkRules = sqliteTable('network_rules', {
  id: text('id').primaryKey(),
  cidr: text('cidr').notNull().unique(),
  defaultRole: text('default_role', { enum: ['member', 'admin'] }).notNull(),
  allowEscalation: integer('allow_escalation', { mode: 'boolean' }).notNull().default(false),
  enabled: integer('enabled', { mode: 'boolean' }).notNull().default(true),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
})

export const collections = sqliteTable('collections', {
  id: text('id').primaryKey(),
  tmdbCollectionId: integer('tmdb_collection_id').notNull().unique(),
  name: text('name').notNull(),
  overview: text('overview'),
  posterPath: text('poster_path'),
  backdropPath: text('backdrop_path'),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
  updatedAt: integer('updated_at', { mode: 'timestamp' }).notNull(),
})

export const metadata = sqliteTable('metadata', {
  id: text('id').primaryKey(),
  provider: text('provider').notNull(),
  type: text('type', { enum: ['movie', 'tv'] }).notNull(),
  title: text('title').notNull(),
  year: integer('year'),
  overview: text('overview'),
  posterPath: text('poster_path'),
  backdropPath: text('backdrop_path'),
  rating: real('rating'),
  tmdbCollectionId: integer('tmdb_collection_id'),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
  updatedAt: integer('updated_at', { mode: 'timestamp' }).notNull(),
})

export const mediaItems = sqliteTable('media_items', {
  id: text('id').primaryKey(),
  path: text('path').notNull().unique(),
  filename: text('filename').notNull(),
  size: integer('size'),
  title: text('title').notNull(),
  year: integer('year'),
  season: integer('season'),
  episode: integer('episode'),
  resolution: text('resolution'),
  source: text('source'),
  videoCodec: text('video_codec'),
  audioCodec: text('audio_codec'),
  hdr: text('hdr'),
  releaseGroup: text('release_group'),
  proper: integer('proper', { mode: 'boolean' }),
  repack: integer('repack', { mode: 'boolean' }),
  metadataId: text('metadata_id').references(() => metadata.id),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
  updatedAt: integer('updated_at', { mode: 'timestamp' }).notNull(),
})

export const requests = sqliteTable('requests', {
  id: text('id').primaryKey(),

  // Media identification
  title: text('title').notNull(),
  year: integer('year'),
  type: text('type', { enum: ['movie', 'tv'] }).notNull(),

  // External IDs for searching
  tmdbId: text('tmdb_id'),
  imdbId: text('imdb_id'),
  tvdbId: text('tvdb_id'),

  // TV-specific
  season: integer('season'),
  seriesStatus: text('series_status', { enum: ['returning', 'ended', 'upcoming', 'unknown'] }),
  monitoringPreset: text('monitoring_preset', {
    enum: ['all', 'future', 'missing', 'latest_season'],
  }),
  totalEpisodes: integer('total_episodes'),
  downloadedEpisodes: integer('downloaded_episodes').default(0),

  // Movie-specific (for calendar)
  releaseDate: text('release_date'),

  // Display
  posterPath: text('poster_path'),
  overview: text('overview'),

  // Request tracking - kept for backwards compatibility, primary user
  requestedBy: text('requested_by').notNull().references(() => users.id),
  status: text('status', {
    enum: [
      'pending_approval',
      'approved',
      'searching',
      'found',
      'fetching_nzb',
      'nzb_downloaded',
      'sent_to_downloader',
      'downloading',
      'download_complete',
      'importing',
      'completed',
      'rejected',
      'failed',
    ],
  }).notNull(),

  // Timestamps
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
  approvedAt: integer('approved_at', { mode: 'timestamp' }),
  approvedBy: text('approved_by').references(() => users.id),
  lastSearchedAt: integer('last_searched_at', { mode: 'timestamp' }),
  completedAt: integer('completed_at', { mode: 'timestamp' }),

  // Search preferences
  minQuality: text('min_quality'),

  // Download tracking
  nzoId: text('nzo_id'),
  releaseName: text('release_name'),
})

// Join table for multiple users requesting the same media
export const requestUsers = sqliteTable('request_users', {
  id: text('id').primaryKey(),
  requestId: text('request_id').notNull().references(() => requests.id),
  userId: text('user_id').notNull().references(() => users.id),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
})

// Episode tracking for TV requests
export const episodes = sqliteTable('episodes', {
  id: text('id').primaryKey(),
  requestId: text('request_id').notNull().references(() => requests.id, { onDelete: 'cascade' }),
  seasonNumber: integer('season_number').notNull(),
  episodeNumber: integer('episode_number').notNull(),
  title: text('title'),
  airDate: text('air_date'),
  status: text('status', {
    enum: ['pending', 'searching', 'downloading', 'completed', 'failed'],
  }).notNull().default('pending'),
  monitored: integer('monitored', { mode: 'boolean' }).notNull().default(true),
  nzoId: text('nzo_id'),
  releaseName: text('release_name'),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
  updatedAt: integer('updated_at', { mode: 'timestamp' }).notNull(),
})
