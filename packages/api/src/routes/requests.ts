import { Hono } from 'hono'
import { requestService, type CreateRequestData, type RequestStatus } from '../services/requests'
import { runSearchJob } from '../jobs/searchRequests'
import type { User } from '../services/auth'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

const requestsRouter = new Hono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

function requireAdmin(user: User | null): user is User {
  return user !== null && user.role === 'admin'
}

// GET /api/requests - List all requests
requestsRouter.get('/', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const status = c.req.query('status') as RequestStatus | undefined
  const type = c.req.query('type') as 'movie' | 'tv' | undefined
  const mine = c.req.query('mine') === 'true'

  const filters: {
    status?: RequestStatus | RequestStatus[]
    userId?: string
    type?: 'movie' | 'tv'
  } = {}

  if (status) {
    filters.status = status
  }

  if (type) {
    filters.type = type
  }

  // Non-admins can only see their own requests by default
  if (mine || user.role !== 'admin') {
    filters.userId = user.id
  }

  const items = await requestService.getAll(filters)

  return c.json({ requests: items })
})

// POST /api/requests - Create new request
requestsRouter.post('/', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const body = (await c.req.json()) as CreateRequestData & { downloading?: boolean }

    if (!body.title || !body.type) {
      return c.json({ error: 'Missing required fields: title, type' }, 400)
    }

    const isAdmin = user.role === 'admin'
    const request = await requestService.create(body, user.id, isAdmin)

    // If this request is being created alongside a download, mark it as downloading
    if (body.downloading) {
      await requestService.updateStatus(request.id, 'downloading')
      const updated = await requestService.getById(request.id)
      return c.json({ success: true, request: updated })
    }

    return c.json({ success: true, request })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to create request' },
      400
    )
  }
})

// GET /api/requests/:id - Get specific request
requestsRouter.get('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const id = c.req.param('id')
  const request = await requestService.getById(id)

  if (!request) {
    return c.json({ error: 'Request not found' }, 404)
  }

  // Non-admins can only see their own requests
  if (user.role !== 'admin' && request.requestedBy.id !== user.id) {
    return c.json({ error: 'Not authorized' }, 403)
  }

  return c.json({ request })
})

// DELETE /api/requests/:id - Delete request
requestsRouter.delete('/:id', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  try {
    const id = c.req.param('id')
    const isAdmin = user.role === 'admin'
    await requestService.delete(id, user.id, isAdmin)

    return c.json({ success: true })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to delete request' },
      400
    )
  }
})

// POST /api/requests/:id/approve - Approve pending request (admin only)
requestsRouter.post('/:id/approve', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    const request = await requestService.approve(id, user.id)

    return c.json({ success: true, request })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to approve request' },
      400
    )
  }
})

// POST /api/requests/:id/reject - Reject pending request (admin only)
requestsRouter.post('/:id/reject', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    const request = await requestService.reject(id, user.id)

    return c.json({ success: true, request })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to reject request' },
      400
    )
  }
})

// POST /api/requests/:id/retry - Retry failed request (admin only)
requestsRouter.post('/:id/retry', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const id = c.req.param('id')
    await requestService.updateStatus(id, 'approved')
    const request = await requestService.getById(id)

    return c.json({ success: true, request })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to retry request' },
      400
    )
  }
})

// POST /api/requests/search-job - Manually trigger search job (admin only)
requestsRouter.post('/search-job', async (c) => {
  const user = c.get('user')
  if (!requireAdmin(user)) {
    return c.json({ error: 'Admin access required' }, 403)
  }

  try {
    const result = await runSearchJob()
    return c.json({ success: true, ...result })
  } catch (error) {
    return c.json(
      { error: error instanceof Error ? error.message : 'Failed to run search job' },
      500
    )
  }
})

export default requestsRouter
