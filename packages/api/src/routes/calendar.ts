import { OpenAPIHono } from '@hono/zod-openapi'
import { and, eq, isNotNull, sql } from 'drizzle-orm'
import { db } from '../db'
import { episodes, requests } from '../db/schema'
import type { User } from '../services/auth'

type AuthEnv = {
  Variables: {
    user: User | null
  }
}

export type CalendarItem = {
  date: string
  type: 'episode' | 'movie'
  title: string
  episode?: {
    season: number
    episode: number
    title: string | null
  }
  status: string
  requestId: string
  posterPath: string | null
}

const calendarRouter = new OpenAPIHono<AuthEnv>()

function requireAuth(user: User | null): user is User {
  return user !== null
}

// GET /api/calendar?start=YYYY-MM-DD&end=YYYY-MM-DD
calendarRouter.get('/', async (c) => {
  const user = c.get('user')
  if (!requireAuth(user)) {
    return c.json({ error: 'Authentication required' }, 401)
  }

  const start = c.req.query('start')
  const end = c.req.query('end')

  if (!start || !end) {
    return c.json({ error: 'start and end query parameters required' }, 400)
  }

  const items: CalendarItem[] = []

  // Get TV episodes with air dates in range
  const episodeResults = await db
    .select({
      airDate: episodes.airDate,
      seasonNumber: episodes.seasonNumber,
      episodeNumber: episodes.episodeNumber,
      episodeTitle: episodes.title,
      episodeStatus: episodes.status,
      requestId: requests.id,
      requestTitle: requests.title,
      posterPath: requests.posterPath,
    })
    .from(episodes)
    .innerJoin(requests, eq(episodes.requestId, requests.id))
    .where(
      and(
        isNotNull(episodes.airDate),
        sql`${episodes.airDate} >= ${start}`,
        sql`${episodes.airDate} <= ${end}`
      )
    )

  for (const ep of episodeResults) {
    if (ep.airDate) {
      items.push({
        date: ep.airDate,
        type: 'episode',
        title: ep.requestTitle,
        episode: {
          season: ep.seasonNumber,
          episode: ep.episodeNumber,
          title: ep.episodeTitle,
        },
        status: ep.episodeStatus,
        requestId: ep.requestId,
        posterPath: ep.posterPath,
      })
    }
  }

  // Get movie requests with release dates
  const movieResults = await db
    .select({
      id: requests.id,
      title: requests.title,
      status: requests.status,
      posterPath: requests.posterPath,
      releaseDate: requests.releaseDate,
    })
    .from(requests)
    .where(
      and(
        eq(requests.type, 'movie'),
        isNotNull(requests.releaseDate),
        sql`${requests.releaseDate} >= ${start}`,
        sql`${requests.releaseDate} <= ${end}`
      )
    )

  for (const movie of movieResults) {
    if (movie.releaseDate) {
      items.push({
        date: movie.releaseDate,
        type: 'movie',
        title: movie.title,
        status: movie.status,
        requestId: movie.id,
        posterPath: movie.posterPath,
      })
    }
  }

  // Sort by date
  items.sort((a, b) => a.date.localeCompare(b.date))

  return c.json({ items })
})

export default calendarRouter
