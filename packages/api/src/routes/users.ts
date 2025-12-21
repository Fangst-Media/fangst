import { Hono } from 'hono'
import { z } from 'zod'
import { zValidator } from '@hono/zod-validator'
import { eq } from 'drizzle-orm'
import { db } from '../db'
import { users } from '../db/schema'
import { configService } from '../services/config'
import type { User } from '../services/auth'

type Variables = {
  user: User | null
}

const usersRouter = new Hono<{ Variables: Variables }>()

const updatePreferencesSchema = z.object({
  preferredRequestMode: z.enum(['simple', 'detailed']),
})

usersRouter.patch('/me/preferences', zValidator('json', updatePreferencesSchema), async (c) => {
  const user = c.get('user')
  if (!user) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const { preferredRequestMode } = c.req.valid('json')

  // Check if user is allowed to change their request mode
  const settings = configService.get()
  const rolePermission =
    user.role === 'admin'
      ? settings.requests.adminRequestMode
      : settings.requests.memberRequestMode

  if (rolePermission !== 'user_choice') {
    return c.json({ error: 'Cannot change request mode with current permissions' }, 403)
  }

  await db.update(users).set({ preferredRequestMode }).where(eq(users.id, user.id))

  return c.json({ success: true, preferredRequestMode })
})

export default usersRouter
