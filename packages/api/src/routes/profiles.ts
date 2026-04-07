import { OpenAPIHono } from '@hono/zod-openapi'
import { profileService } from '../services/profiles'
import type { CreateProfile, UpdateProfile } from '@jaeger/shared/types/profiles'
import type { User } from '../services/auth'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const profiles = new OpenAPIHono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

function requireAdmin(user: User | null): user is User {
  return user !== null && user.role === 'admin'
}

// GET /api/profiles - List all profiles (any authenticated user)
profiles.get('/', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const all = await profileService.getAll()
  return c.json({ profiles: all })
})

// GET /api/profiles/:id - Get profile by ID
profiles.get('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const profile = await profileService.getById(c.req.param('id'))
  if (!profile) {
    return c.json({ error: 'Profile not found' }, 404)
  }

  return c.json({ profile })
})

// POST /api/profiles - Create profile (admin only)
profiles.post('/', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const body = (await c.req.json()) as CreateProfile
    if (!body.name) {
      return c.json({ error: 'Name is required' }, 400)
    }

    const profile = await profileService.create(body)
    return c.json({ success: true, profile })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to create profile' },
      400
    )
  }
})

// PATCH /api/profiles/:id - Update profile (admin only)
profiles.patch('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    const body = (await c.req.json()) as UpdateProfile
    const profile = await profileService.update(id, body)
    return c.json({ success: true, profile })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to update profile' },
      400
    )
  }
})

// DELETE /api/profiles/:id - Delete profile (admin only)
profiles.delete('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    await profileService.delete(id)
    return c.json({ success: true })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to delete profile' },
      400
    )
  }
})

export default profiles
