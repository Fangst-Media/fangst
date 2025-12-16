import { integer, sqliteTable, text } from 'drizzle-orm/sqlite-core'

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
  email: text('email').unique(),
  name: text('name'),
  role: text('role', { enum: ['browse', 'request', 'member', 'admin'] }).notNull().default('browse'),
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
  defaultRole: text('default_role', { enum: ['browse', 'request', 'member', 'admin'] }).notNull(),
  allowEscalation: integer('allow_escalation', { mode: 'boolean' }).notNull().default(false),
  enabled: integer('enabled', { mode: 'boolean' }).notNull().default(true),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull(),
})
