import { Hono } from 'hono'
import { configService } from '../services/config'
import type { SettingsUpdate, SettingsResponse } from '@jaeger/shared/types/settings'
import type { User } from '../services/auth'
import { logger } from '../lib/logger'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const settings = new Hono<AuthEnv>()

function maskApiKey(key: string): string {
  if (key.length <= 8) return '********'
  return `${key.slice(0, 4)}********${key.slice(-4)}`
}

async function validateTmdbApiKey(apiKey: string): Promise<boolean> {
  try {
    const response = await fetch(
      `https://api.themoviedb.org/3/configuration?api_key=${apiKey}`,
      { signal: AbortSignal.timeout(5000) }
    )
    return response.ok
  } catch (error) {
    logger.error({ error }, 'Failed to validate TMDB API key')
    return false
  }
}

async function validateTvdbApiKey(apiKey: string): Promise<boolean> {
  try {
    const response = await fetch('https://api4.thetvdb.com/v4/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ apikey: apiKey }),
      signal: AbortSignal.timeout(5000),
    })
    return response.ok
  } catch (error) {
    logger.error({ error }, 'Failed to validate TVDB API key')
    return false
  }
}

// GET /api/settings - Get all settings with lock status (admin only)
settings.get('/', async (c) => {
  const user = c.get('user')
  if (!user || user.role !== 'admin') {
    return c.json({ error: 'Admin access required' }, 403)
  }

  const settingsData = configService.get()
  const locked = configService.getLocked()

  // Mask sensitive values
  const maskedSettings = {
    ...settingsData,
    security: {
      ...settingsData.security,
      apiKey: maskApiKey(settingsData.security.apiKey),
    },
    metadata: {
      ...settingsData.metadata,
      tmdb: {
        ...settingsData.metadata.tmdb,
        apiKey: settingsData.metadata.tmdb.apiKey ? '********' : '',
      },
      tvdb: {
        ...settingsData.metadata.tvdb,
        apiKey: settingsData.metadata.tvdb.apiKey ? '********' : '',
      },
    },
  }

  const response: SettingsResponse = {
    settings: maskedSettings,
    locked,
  }

  return c.json(response)
})

// GET /api/settings/ui - Public UI settings (no auth required)
settings.get('/ui', async (c) => {
  const settingsData = configService.get()
  return c.json({ ui: settingsData.ui })
})

// PATCH /api/settings - Update settings (admin only)
settings.patch('/', async (c) => {
  const user = c.get('user')
  if (!user || user.role !== 'admin') {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const updates = (await c.req.json()) as SettingsUpdate

    // Validate TMDB API key if it's being updated
    if (updates.metadata?.tmdb?.apiKey) {
      const isValid = await validateTmdbApiKey(updates.metadata.tmdb.apiKey)
      if (!isValid) {
        logger.warn('Invalid TMDB API key provided in settings update')
        return c.json(
          {
            error:
              'Invalid TMDB API key. Please verify your key at https://www.themoviedb.org/settings/api',
          },
          400
        )
      }
      logger.info('TMDB API key validated successfully')
    }

    // Validate TVDB API key if it's being updated
    if (updates.metadata?.tvdb?.apiKey) {
      const isValid = await validateTvdbApiKey(updates.metadata.tvdb.apiKey)
      if (!isValid) {
        logger.warn('Invalid TVDB API key provided in settings update')
        return c.json(
          {
            error:
              'Invalid TVDB API key. Please verify your key at https://thetvdb.com/dashboard/account/apikeys',
          },
          400
        )
      }
      logger.info('TVDB API key validated successfully')
    }

    const newSettings = await configService.update(updates)
    const locked = configService.getLocked()

    // Mask sensitive values in response
    const maskedSettings = {
      ...newSettings,
      security: {
        ...newSettings.security,
        apiKey: maskApiKey(newSettings.security.apiKey),
      },
      metadata: {
        ...newSettings.metadata,
        tmdb: {
          ...newSettings.metadata.tmdb,
          apiKey: newSettings.metadata.tmdb.apiKey ? '********' : '',
        },
        tvdb: {
          ...newSettings.metadata.tvdb,
          apiKey: newSettings.metadata.tvdb.apiKey ? '********' : '',
        },
      },
    }

    return c.json({
      success: true,
      settings: maskedSettings,
      locked,
    })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to update settings' },
      400
    )
  }
})

// GET /api/settings/api-key - Get full API key (admin only)
settings.get('/api-key', async (c) => {
  const user = c.get('user')
  if (!user || user.role !== 'admin') {
    return c.json({ error: 'Admin access required' }, 403)
  }

  const settingsData = configService.get()
  const isLocked = configService.isLocked('security.apiKey')

  return c.json({
    apiKey: settingsData.security.apiKey,
    locked: isLocked,
  })
})

// POST /api/settings/api-key/regenerate - Regenerate API key (admin only)
settings.post('/api-key/regenerate', async (c) => {
  const user = c.get('user')
  if (!user || user.role !== 'admin') {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const newKey = await configService.regenerateApiKey()
    return c.json({
      success: true,
      apiKey: newKey,
    })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to regenerate API key' },
      400
    )
  }
})

export default settings
