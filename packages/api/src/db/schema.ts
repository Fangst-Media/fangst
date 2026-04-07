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
  type: text('type', { enum: ['movie', 'tv', 'book'] }).notNull(),

  // External IDs for searching
  tmdbId: text('tmdb_id'),
  imdbId: text('imdb_id'),
  tvdbId: text('tvdb_id'),
  openLibraryId: text('open_library_id'),

  // Book-specific
  author: text('author'),
  format: text('format', { enum: ['ebook', 'audiobook'] }),

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
  qualityProfileId: text('quality_profile_id').references(() => qualityProfiles.id),
  mode: text('mode', { enum: ['auto', 'manual'] }).notNull().default('auto'),

  // Download tracking (deprecated — moving to downloads table)
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

// --- Metadata Store ---

export const movieMetadata = sqliteTable('movie_metadata', {
  id: text('id').primaryKey(),
  provider: text('provider').notNull(),
  title: text('title').notNull(),
  year: integer('year'),
  overview: text('overview'),
  posterPath: text('poster_path'),
  backdropPath: text('backdrop_path'),
  rating: real('rating'),
  runtime: integer('runtime'),
  imdbId: text('imdb_id'),
  genres: text('genres'),
  collectionJson: text('collection_json'),
  releaseDate: text('release_date'),
  lastRefreshedAt: integer('last_refreshed_at', { mode: 'timestamp' }).notNull(),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
})

export const tvMetadata = sqliteTable('tv_metadata', {
  id: text('id').primaryKey(),
  provider: text('provider').notNull(),
  title: text('title').notNull(),
  year: integer('year'),
  overview: text('overview'),
  posterPath: text('poster_path'),
  backdropPath: text('backdrop_path'),
  rating: real('rating'),
  status: text('status', { enum: ['returning', 'ended', 'upcoming', 'unknown'] }),
  genres: text('genres'),
  numberOfSeasons: integer('number_of_seasons'),
  seasonsJson: text('seasons_json'),
  lastRefreshedAt: integer('last_refreshed_at', { mode: 'timestamp' }).notNull(),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
})

export const bookMetadata = sqliteTable('book_metadata', {
  id: text('id').primaryKey(),
  provider: text('provider').notNull(),
  title: text('title').notNull(),
  author: text('author'),
  authorId: text('author_id'),
  year: integer('year'),
  overview: text('overview'),
  coverUrl: text('cover_url'),
  subjects: text('subjects'),
  isbn: text('isbn'),
  pageCount: integer('page_count'),
  lastRefreshedAt: integer('last_refreshed_at', { mode: 'timestamp' }).notNull(),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
})

export const metadataLists = sqliteTable('metadata_lists', {
  id: text('id').primaryKey(),
  listType: text('list_type').notNull(),
  mediaType: text('media_type').notNull(),
  provider: text('provider'),
  resultsJson: text('results_json').notNull(),
  lastRefreshedAt: integer('last_refreshed_at', { mode: 'timestamp' }).notNull(),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
})

// Quality profiles for release selection
export const qualityProfiles = sqliteTable('quality_profiles', {
  id: text('id').primaryKey(),
  name: text('name').notNull(),
  // Video
  minResolution: text('min_resolution', { enum: ['720p', '1080p', '2160p'] }).notNull().default('720p'),
  preferredResolution: text('preferred_resolution', { enum: ['720p', '1080p', '2160p'] }),
  preferredSources: text('preferred_sources'), // JSON: ['remux','bluray','webdl']
  // HDR
  requireHdr: integer('require_hdr', { mode: 'boolean' }).notNull().default(false),
  preferredHdr: text('preferred_hdr'), // JSON: ['dv','hdr10+','hdr10']
  // Audio
  preferredAudio: text('preferred_audio'), // JSON: ['atmos','truehd','dtshd']
  // Size filter (overrides global when set)
  sizeFilterEnabled: integer('size_filter_enabled', { mode: 'boolean' }),
  sizeFilterMinMbPerMin: real('size_filter_min_mb_per_min'),
  // Upgrade behavior
  cutoffScore: integer('cutoff_score').notNull().default(80),
  upgradeAllowed: integer('upgrade_allowed', { mode: 'boolean' }).notNull().default(false),
  // Metadata
  isDefault: integer('is_default', { mode: 'boolean' }).notNull().default(false),
  sortOrder: integer('sort_order').notNull().default(0),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
  updatedAt: integer('updated_at', { mode: 'timestamp' }).notNull(),
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
  // Deprecated — moving to downloads table
  nzoId: text('nzo_id'),
  releaseName: text('release_name'),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
  updatedAt: integer('updated_at', { mode: 'timestamp' }).notNull(),
})

// Download attempts (fulfillment of requests)
export const downloads = sqliteTable('downloads', {
  id: text('id').primaryKey(),
  requestId: text('request_id').notNull().references(() => requests.id, { onDelete: 'cascade' }),
  episodeId: text('episode_id').references(() => episodes.id, { onDelete: 'cascade' }),
  // Release info
  releaseName: text('release_name').notNull(),
  indexer: text('indexer'),
  size: integer('size'),
  // Parsed quality (denormalized for history)
  resolution: text('resolution'),
  source: text('source'),
  videoCodec: text('video_codec'),
  audioCodec: text('audio_codec'),
  hdr: text('hdr'),
  releaseGroup: text('release_group'),
  qualityScore: integer('quality_score'),
  // Download tracking
  nzoId: text('nzo_id'),
  status: text('status', {
    enum: ['fetching', 'downloading', 'complete', 'failed', 'cancelled', 'replaced'],
  }).notNull(),
  failReason: text('fail_reason'),
  replacedById: text('replaced_by_id'),
  // Timestamps
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
  completedAt: integer('completed_at', { mode: 'timestamp' }),
})
