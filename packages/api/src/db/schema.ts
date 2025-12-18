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
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
  updatedAt: integer('updated_at', { mode: 'timestamp' }).notNull(),
})

export const mediaItems = sqliteTable('media_items', {
  id: text('id').primaryKey(),
  path: text('path').notNull().unique(),
  filename: text('filename').notNull(),
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
