// Sonarr and Radarr share the v3 API for download clients, indexers, profiles, commands and
// history; they differ in how a series or movie is added and what counts as imported.

import {
  type Json,
  type JsonObject,
  type Protocol,
  type Release,
  bool,
  isObject,
  list,
  num,
  objects,
  str,
} from '../shared'
import { type Api, HttpError, call, log, waitFor } from './http'
import { handedOver } from './records'

export type ArrApp = 'sonarr' | 'radarr'

export interface ArrConfig {
  app: ArrApp
  api: Api
  /** Port of this app's recording proxy in front of Fangst. */
  recorderPort: number
  fangstApiKey: string
  protocols: Protocol[]
}

export const clientName: Record<Protocol, string> = {
  usenet: 'Fangst (SABnzbd)',
  torrent: 'Fangst (qBittorrent)',
}

const IMPORT_TIMEOUT_MS = 10 * 60_000

function withFields(schema: JsonObject, values: Record<string, Json>): JsonObject {
  const fields = objects(schema, 'fields').map((f) => {
    const name = str(f, 'name')
    return name !== undefined && Object.hasOwn(values, name)
      ? { ...f, value: values[name] ?? null }
      : f
  })
  return { ...schema, fields }
}

async function schema(cfg: ArrConfig, kind: string, implementation: string): Promise<JsonObject> {
  const all = objects(await call(cfg.api, 'GET', `/api/v3/${kind}/schema`))
  const found = all.find((s) => str(s, 'implementation') === implementation)
  if (found === undefined) throw new Error(`${cfg.app} has no ${kind} called ${implementation}`)
  return found
}

/** Runs the app's own connection test, then saves. The test must pass; its failures are the
 * validation messages the app would show next to the form. */
async function testAndAdd(cfg: ArrConfig, kind: string, payload: JsonObject): Promise<number> {
  const name = str(payload, 'name') ?? kind
  try {
    await call(cfg.api, 'POST', `/api/v3/${kind}/test`, payload)
  } catch (e) {
    if (e instanceof HttpError)
      throw new Error(`${cfg.app}: the ${kind} test for ${name} failed: ${e.body}`)
    throw e
  }
  log(cfg.app, `${kind} test passed: ${name}`)
  const saved = await call(cfg.api, 'POST', `/api/v3/${kind}`, payload)
  const id = num(saved, 'id')
  if (id === undefined) throw new Error(`${cfg.app}: saving ${name} returned no id`)
  return id
}

export async function waitUntilUp(cfg: ArrConfig): Promise<void> {
  const status = await waitFor(`${cfg.app} to start`, 5 * 60_000, async () => {
    const answer = await call(cfg.api, 'GET', '/api/v3/system/status')
    return isObject(answer) ? answer : undefined
  })
  log(cfg.app, `up, version ${str(status, 'version')}`)
}

export async function version(cfg: ArrConfig): Promise<string> {
  return str(await call(cfg.api, 'GET', '/api/v3/system/status'), 'version') ?? 'unknown'
}

export async function configure(cfg: ArrConfig): Promise<void> {
  const tv = cfg.app === 'sonarr'
  await call(cfg.api, 'POST', '/api/v3/rootfolder', { path: tv ? '/data/tv' : '/data/movies' })

  if (cfg.protocols.includes('usenet')) {
    const sab = await schema(cfg, 'downloadclient', 'Sabnzbd')
    await testAndAdd(
      cfg,
      'downloadclient',
      withFields(
        {
          ...sab,
          name: clientName.usenet,
          enable: true,
          priority: 1,
          removeCompletedDownloads: true,
          removeFailedDownloads: true,
        },
        {
          host: 'recorder',
          port: cfg.recorderPort,
          useSsl: false,
          urlBase: '',
          apiKey: cfg.fangstApiKey,
        }
      )
    )
    const newznab = await schema(cfg, 'indexer', 'Newznab')
    await testAndAdd(
      cfg,
      'indexer',
      withFields(
        {
          ...newznab,
          name: 'Fangst e2e (Newznab)',
          enableRss: true,
          enableAutomaticSearch: true,
          enableInteractiveSearch: true,
          priority: 25,
        },
        {
          baseUrl: 'http://indexer:9117/newznab',
          apiPath: '/api',
          apiKey: cfg.app,
          categories: tv ? [5000, 5040] : [2000, 2040],
        }
      )
    )
  }
  if (cfg.protocols.includes('torrent')) {
    const qbit = await schema(cfg, 'downloadclient', 'QBittorrent')
    // Sonarr and Radarr only offer username and password; Fangst takes an API key as the password.
    await testAndAdd(
      cfg,
      'downloadclient',
      withFields(
        {
          ...qbit,
          name: clientName.torrent,
          enable: true,
          priority: 1,
          removeCompletedDownloads: true,
          removeFailedDownloads: true,
        },
        {
          host: 'recorder',
          port: cfg.recorderPort,
          useSsl: false,
          urlBase: '',
          username: 'fangst',
          password: cfg.fangstApiKey,
        }
      )
    )
    const torznab = await schema(cfg, 'indexer', 'Torznab')
    await testAndAdd(
      cfg,
      'indexer',
      withFields(
        {
          ...torznab,
          name: 'Fangst e2e (Torznab)',
          enableRss: true,
          enableAutomaticSearch: true,
          enableInteractiveSearch: true,
          priority: 25,
        },
        {
          baseUrl: 'http://indexer:9117/torznab',
          apiPath: '/api',
          apiKey: cfg.app,
          categories: tv ? [5000, 5040] : [2000, 2040],
          minimumSeeders: 1,
        }
      )
    )
  }
}

async function profileId(cfg: ArrConfig): Promise<number> {
  const profiles = objects(await call(cfg.api, 'GET', '/api/v3/qualityprofile'))
  const any = profiles.find((p) => str(p, 'name') === 'Any') ?? profiles[0]
  const id = num(any, 'id')
  if (id === undefined) throw new Error(`${cfg.app} has no quality profile`)
  return id
}

/** What one release turned into in the app: the episode or movie to search for and check. */
interface Target {
  release: Release
  /** Episode id for Sonarr, movie id for Radarr. */
  id: number
}

async function addSeries(cfg: ArrConfig, releases: Release[]): Promise<Target[]> {
  const tvdbId = releases[0]?.tvdbId
  if (tvdbId === undefined) return []
  const found = objects(await call(cfg.api, 'GET', `/api/v3/series/lookup?term=tvdb:${tvdbId}`))[0]
  if (found === undefined) throw new Error(`sonarr found no series for tvdb ${tvdbId}`)
  const series = await call(cfg.api, 'POST', '/api/v3/series', {
    ...found,
    qualityProfileId: await profileId(cfg),
    rootFolderPath: '/data/tv',
    monitored: true,
    seasonFolder: true,
    addOptions: {
      monitor: 'none',
      searchForMissingEpisodes: false,
      searchForCutoffUnmetEpisodes: false,
    },
  })
  const seriesId = num(series, 'id')
  if (seriesId === undefined) throw new Error('sonarr returned no series id')
  log(cfg.app, `added series ${str(series, 'title')} (${seriesId})`)
  const episodes = await waitFor('sonarr to load the episode list', 3 * 60_000, async () => {
    const list = objects(await call(cfg.api, 'GET', `/api/v3/episode?seriesId=${seriesId}`))
    return list.length > 0 ? list : undefined
  })
  const targets = releases.map((release) => {
    const episode = episodes.find(
      (e) =>
        num(e, 'seasonNumber') === release.season && num(e, 'episodeNumber') === release.episode
    )
    const id = num(episode, 'id')
    if (id === undefined) throw new Error(`sonarr has no episode for ${release.title}`)
    return { release, id }
  })
  await call(cfg.api, 'PUT', '/api/v3/episode/monitor', {
    episodeIds: targets.map((t) => t.id),
    monitored: true,
  })
  return targets
}

async function addMovie(cfg: ArrConfig, release: Release): Promise<Target> {
  const found = await call(cfg.api, 'GET', `/api/v3/movie/lookup/tmdb?tmdbId=${release.tmdbId}`)
  if (!isObject(found)) throw new Error(`radarr found no movie for tmdb ${release.tmdbId}`)
  const movie = await call(cfg.api, 'POST', '/api/v3/movie', {
    ...found,
    qualityProfileId: await profileId(cfg),
    rootFolderPath: '/data/movies',
    monitored: true,
    minimumAvailability: 'released',
    addOptions: { searchForMovie: false, monitor: 'movieOnly' },
  })
  const id = num(movie, 'id')
  if (id === undefined) throw new Error('radarr returned no movie id')
  log(cfg.app, `added movie ${str(movie, 'title')} (${id})`)
  return { release, id }
}

export async function addTargets(cfg: ArrConfig, releases: Release[]): Promise<Target[]> {
  if (cfg.app === 'sonarr') return addSeries(cfg, releases)
  const targets: Target[] = []
  for (const release of releases) targets.push(await addMovie(cfg, release))
  return targets
}

async function imported(cfg: ArrConfig, target: Target): Promise<boolean> {
  const path = cfg.app === 'sonarr' ? `/api/v3/episode/${target.id}` : `/api/v3/movie/${target.id}`
  return bool(await call(cfg.api, 'GET', path), 'hasFile') === true
}

/** Queue entries with a warning or error, to say why an import is stuck. */
async function queueProblems(cfg: ArrConfig): Promise<string[]> {
  const queue = await call(
    cfg.api,
    'GET',
    '/api/v3/queue?pageSize=50&includeUnknownSeriesItems=true&includeUnknownMovieItems=true'
  )
  return objects(queue, 'records')
    .filter((r) => str(r, 'trackedDownloadStatus') !== 'ok' || str(r, 'status') === 'warning')
    .map((r) => {
      const messages = objects(r, 'statusMessages')
        .flatMap((m) => [
          str(m, 'title') ?? '',
          ...list(m, 'messages').filter((x) => typeof x === 'string'),
        ])
        .join('; ')
      return `${str(r, 'title')}: ${str(r, 'status')}/${str(r, 'trackedDownloadState')} ${str(r, 'errorMessage') ?? ''} ${messages}`
    })
}

export interface Outcome {
  app: string
  release: string
  protocol: Protocol
  ok: boolean
  detail: string
}

export async function grabAndImport(cfg: ArrConfig, target: Target): Promise<Outcome> {
  const { release } = target
  const command: JsonObject =
    cfg.app === 'sonarr'
      ? { name: 'EpisodeSearch', episodeIds: [target.id] }
      : { name: 'MoviesSearch', movieIds: [target.id] }
  await call(cfg.api, 'POST', '/api/v3/command', command)
  log(cfg.app, `searching for ${release.title}`)
  let lastProblems: string[] = []
  try {
    await waitFor(
      `${cfg.app} to import ${release.title}`,
      IMPORT_TIMEOUT_MS,
      async () => {
        if (await imported(cfg, target)) return true
        const problems = await queueProblems(cfg)
        if (problems.join() !== lastProblems.join()) {
          for (const p of problems) log(cfg.app, `queue: ${p}`)
          lastProblems = problems
        }
        return undefined
      },
      5000
    )
  } catch (e) {
    return {
      app: cfg.app,
      release: release.title,
      protocol: release.protocol,
      ok: false,
      detail: String(e),
    }
  }
  const grabbedBy = await grabbingClient(cfg, target)
  const via = await handedOver(cfg.app, release.title)
  const ok = grabbedBy === clientName[release.protocol] && via === release.protocol
  log(cfg.app, `imported ${release.title} via ${grabbedBy}`)
  return {
    app: cfg.app,
    release: release.title,
    protocol: release.protocol,
    ok,
    detail: `imported; grabbed by ${grabbedBy}, handed to Fangst over ${via ?? 'nothing recorded'}`,
  }
}

async function grabbingClient(cfg: ArrConfig, target: Target): Promise<string | undefined> {
  const key = cfg.app === 'sonarr' ? 'episodeId' : 'movieId'
  const history = await call(
    cfg.api,
    'GET',
    '/api/v3/history?page=1&pageSize=200&sortKey=date&sortDirection=descending'
  )
  const grabbed = objects(history, 'records').find(
    (r) => num(r, key) === target.id && str(r, 'eventType') === 'grabbed'
  )
  const data = grabbed?.data
  return str(data, 'downloadClientName') ?? str(data, 'downloadClient')
}
