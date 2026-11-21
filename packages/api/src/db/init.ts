import { existsSync, mkdirSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { $ } from 'bun'

/**
 * Initialize database by ensuring the data directory exists and schema is applied.
 * Runs automatically on application startup.
 */
export async function initializeDatabase(): Promise<void> {
  const dbPath = resolve(process.cwd(), process.env.DATABASE_URL || './data/fangst.db')
  const dataDir = dirname(dbPath)

  // Create data directory if it doesn't exist
  if (!existsSync(dataDir)) {
    mkdirSync(dataDir, { recursive: true })
  }

  // Apply schema using drizzle-kit push
  // Using force flag to skip confirmation prompt
  try {
    await $`drizzle-kit push --force`.quiet()
  } catch (error) {
    console.error('[Database] Failed to initialize database schema:', error)
    throw error
  }
}
