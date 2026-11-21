import { Database } from 'bun:sqlite'
import { drizzle } from 'drizzle-orm/bun-sqlite'
import type { BunSQLiteDatabase } from 'drizzle-orm/bun-sqlite'
import * as schema from './schema'

let _db: BunSQLiteDatabase<typeof schema> | null = null

export function getDb(): BunSQLiteDatabase<typeof schema> {
  if (!_db) {
    const sqlite = new Database(process.env.DATABASE_URL || './data/fangst.db')
    _db = drizzle(sqlite, { schema })
  }
  return _db
}

// Export db for backwards compatibility, but it's now lazy-loaded
export const db = new Proxy({} as BunSQLiteDatabase<typeof schema>, {
  get: (_, prop) => {
    return getDb()[prop as keyof BunSQLiteDatabase<typeof schema>]
  },
})
