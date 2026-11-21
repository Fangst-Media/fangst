// HolyArrs has no API key until an owner exists: the driver makes the owner over HTTP (the first
// caller becomes owner), mints an API key with that fresh session, and uses the key from then on.

import {
  type JsonObject,
  type Protocol,
  type Release,
  isObject,
  num,
  objects,
  str,
} from '../shared'
import type { Outcome } from './arr'
import { type Api, GiveUp, HttpError, call, log, waitFor } from './http'
import { handedOver } from './records'

const BASE = 'http://holyarrs:6262'
const OWNER = { name: 'admin', password: 'e2e-holyarrs-owner-password' }
const IMPORT_TIMEOUT_MS = 10 * 60_000

async function apiKey(): Promise<string> {
  const anonymous: Api = { base: BASE, headers: {} }
  const state = await waitFor('HolyArrs to start', 5 * 60_000, async () => {
    const answer = await call(anonymous, 'GET', '/api/v1/auth/state')
    return isObject(answer) ? answer : undefined
  })
  if (state.needs_setup !== true)
    throw new Error('HolyArrs already has an owner; start from empty volumes')
  const response = await fetch(`${BASE}/api/v1/auth/setup`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(OWNER),
  })
  const created: unknown = await response.json()
  const csrf = str(created, 'csrf')
  const cookie = response.headers
    .getSetCookie()
    .map((c) => c.split(';')[0])
    .find((c) => c?.startsWith('holyarrs_session='))
  if (!response.ok || csrf === undefined || cookie === undefined) {
    throw new Error(
      `creating the HolyArrs owner failed: ${response.status} ${JSON.stringify(created)}`
    )
  }
  // Minting a key needs a recently proven session, which the one setup made is.
  const key = await call(
    { base: BASE, headers: { cookie, 'x-csrf-token': csrf } },
    'POST',
    '/api/v1/auth/keys',
    { label: 'fangst-e2e', scopes: ['all'] }
  )
  const value = str(key, 'key')
  if (value === undefined) throw new Error(`HolyArrs minted no key: ${JSON.stringify(key)}`)
  return value
}

function idOf(value: unknown, ...nested: string[]): number {
  for (const key of nested) {
    const id = num(isObject(value) ? value[key] : undefined, 'id')
    if (id !== undefined) return id
  }
  const id = num(value, 'id')
  if (id === undefined) throw new Error(`no id in ${JSON.stringify(value).slice(0, 500)}`)
  return id
}

/** Creates a downloader or Source, then runs HolyArrs' own test, which must answer ok. */
async function addAndTest(
  api: Api,
  kind: 'downloaders' | 'sources',
  body: JsonObject
): Promise<void> {
  const created = await call(api, 'POST', `/api/v1/${kind}`, body)
  const id = idOf(created, 'downloader', 'source')
  const test = await call(api, 'POST', `/api/v1/${kind}/${id}/test`)
  if (!isObject(test) || test.ok !== true) {
    throw new Error(
      `HolyArrs ${kind} test for ${String(body.name)} failed: ${JSON.stringify(test)}`
    )
  }
  log('holyarrs', `${kind} test passed: ${String(body.name)} (${str(test, 'text') ?? ''})`)
}

/** HolyArrs answers 503 while it still counts the metadata service as down, which it does for a
 * while after a lookup failed at startup. */
async function addTitle(api: Api, path: string, body: JsonObject): Promise<unknown> {
  return waitFor(`HolyArrs to add ${String(body.id)}`, 3 * 60_000, async () => {
    try {
      return await call(api, 'POST', path, body)
    } catch (e) {
      if (e instanceof HttpError && e.status === 503) return undefined
      throw e
    }
  })
}

async function folder(api: Api, path: string, kind: 'movie' | 'show'): Promise<number> {
  return idOf(
    await call(api, 'POST', '/api/v1/library-folders', { path, kind, label: kind }),
    'folder'
  )
}

/** Slot states that will not turn into an import without a person. */
const stuck = new Set(['failed', 'rejected', 'needs_a_decision'])

function slotSummary(slots: JsonObject[]): string {
  return slots
    .map(
      (s) =>
        `${str(s, 'state')}${str(s, 'reason') ? ` (${str(s, 'reason')})` : ''} ${JSON.stringify(s.download ?? '')}`
    )
    .join('; ')
}

async function waitForSlot(what: string, slots: () => Promise<JsonObject[]>): Promise<void> {
  let last = ''
  await waitFor(
    what,
    IMPORT_TIMEOUT_MS,
    async () => {
      const current = await slots()
      const summary = slotSummary(current)
      if (summary !== last) {
        log('holyarrs', `${what}: ${summary}`)
        last = summary
      }
      if (current.some((s) => str(s, 'state') === 'in_library')) return true
      if (current.length > 0 && current.every((s) => stuck.has(str(s, 'state') ?? ''))) {
        throw new GiveUp(`stuck: ${summary}`)
      }
      return undefined
    },
    5000
  )
}

async function movie(api: Api, release: Release, folderId: number): Promise<void> {
  const added = await addTitle(api, '/api/v1/titles', {
    id: `tmdb:${release.tmdbId}`,
    library_folder_id: folderId,
    search_on_add: false,
  })
  const titleId = idOf(added, 'title')
  log('holyarrs', `added movie ${release.tmdbId} as title ${titleId}`)
  await call(api, 'POST', `/api/v1/titles/${titleId}/search`, {})
  await waitForSlot(`import of ${release.title}`, async () => {
    const title = await call(api, 'GET', `/api/v1/titles/${titleId}`)
    return objects(field(title, 'title') ?? title, 'slots')
  })
}

function field(value: unknown, key: string): JsonObject | undefined {
  const found = isObject(value) ? value[key] : undefined
  return isObject(found) ? found : undefined
}

async function episode(api: Api, release: Release, folderId: number): Promise<void> {
  const added = await addTitle(api, '/api/v1/shows', {
    id: `tvdb:${release.tvdbId}`,
    library_folder_id: folderId,
    details_source: 'tmdb',
    seasons: 'all',
  })
  const titleId = idOf(added, 'title', 'show')
  log('holyarrs', `added show ${release.tvdbId} as title ${titleId}`)
  const episodes = async (): Promise<JsonObject[]> =>
    objects(await call(api, 'GET', `/api/v1/titles/${titleId}/episodes?files=1`), 'episodes')
  const target = await waitFor('HolyArrs to list the episodes', 3 * 60_000, async () =>
    (await episodes()).find(
      (e) => num(e, 'season') === release.season && num(e, 'episode') === release.episode
    )
  )
  const episodeId = idOf(target)
  await call(api, 'POST', `/api/v1/titles/${titleId}/search`, { item_id: episodeId })
  await waitForSlot(`import of ${release.title}`, async () =>
    objects(
      (await episodes()).find((e) => num(e, 'id') === episodeId),
      'slots'
    )
  )
}

export async function runHolyarrs(
  releases: Release[],
  protocols: Protocol[],
  fangstApiKey: string
): Promise<{ outcomes: Outcome[]; version: string }> {
  const key = await apiKey()
  const api: Api = { base: BASE, headers: { 'x-api-key': key } }
  const me = await call(api, 'GET', '/api/v1/system/status').catch(() => null)
  const version = str(me, 'version') ?? process.env.HOLYARRS_VERSION ?? 'unknown'
  log('holyarrs', `owner made, API key minted, version ${version}`)
  // The test video is 80 KB under a release that claims gigabytes, which HolyArrs would rightly
  // call a fake.
  await call(api, 'PUT', '/api/v1/settings/search', {
    refuse_tiny_releases: false,
    import_size_floors: false,
  })
  const movies = await folder(api, '/data/holyarrs/movies', 'movie')
  const shows = await folder(api, '/data/holyarrs/tv', 'show')
  const sourceBase = {
    allow_local_network: true,
    categories: { movie: [2000], tv: [5000] },
    kinds: ['movie', 'tv'],
  }
  if (protocols.includes('usenet')) {
    await addAndTest(api, 'downloaders', {
      name: 'Fangst (SABnzbd)',
      kind: 'sabnzbd',
      url: 'http://recorder:8083',
      secret: fangstApiKey,
      category: 'movies',
      category_shows: 'tv',
      allow_local_network: true,
    })
    await addAndTest(api, 'sources', {
      ...sourceBase,
      name: 'Fangst e2e (Newznab)',
      format: 'newznab',
      url: 'http://indexer:9117/newznab/api',
      api_key: 'holyarrs',
    })
  }
  if (protocols.includes('torrent')) {
    await addAndTest(api, 'downloaders', {
      name: 'Fangst (qBittorrent)',
      kind: 'qbittorrent',
      url: 'http://recorder:8083',
      username: 'fangst',
      secret: fangstApiKey,
      category: 'holyarrs',
      allow_local_network: true,
    })
    await addAndTest(api, 'sources', {
      ...sourceBase,
      name: 'Fangst e2e (Torznab)',
      format: 'torznab',
      url: 'http://indexer:9117/torznab/api',
      api_key: 'holyarrs',
    })
  }
  const outcomes: Outcome[] = []
  for (const release of releases.filter(
    (r) => r.app === 'holyarrs' && protocols.includes(r.protocol)
  )) {
    try {
      if (release.tmdbId !== undefined) await movie(api, release, movies)
      else await episode(api, release, shows)
      const via = await handedOver('holyarrs', release.title)
      outcomes.push({
        app: 'holyarrs',
        release: release.title,
        protocol: release.protocol,
        ok: via === release.protocol,
        detail: `imported; handed to Fangst over ${via ?? 'nothing recorded'}`,
      })
    } catch (e) {
      const detail = e instanceof HttpError ? e.message : String(e)
      log('holyarrs', `${release.title} failed: ${detail}`)
      outcomes.push({
        app: 'holyarrs',
        release: release.title,
        protocol: release.protocol,
        ok: false,
        detail,
      })
    }
  }
  return { outcomes, version }
}
