// Types and JSON narrowing shared by the stub server, the driver and the fixture writer.

export type App = 'sonarr' | 'radarr' | 'holyarrs'
export type Protocol = 'usenet' | 'torrent'

export interface Release {
  app: App
  protocol: Protocol
  title: string
  tvdbId?: number
  season?: number
  episode?: number
  tmdbId?: number
  imdbId?: string
  /** What the indexer claims. The real file is a 16 KB video, which every client's minimum
   * size for 1080p would refuse. */
  reportedBytes: number
}

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json }
export type JsonObject = { [key: string]: Json }

export function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function field(value: unknown, key: string): Json | undefined {
  return isObject(value) ? value[key] : undefined
}

export function str(value: unknown, key: string): string | undefined {
  const found = field(value, key)
  return typeof found === 'string' ? found : undefined
}

export function num(value: unknown, key: string): number | undefined {
  const found = field(value, key)
  return typeof found === 'number' ? found : undefined
}

export function bool(value: unknown, key: string): boolean | undefined {
  const found = field(value, key)
  return typeof found === 'boolean' ? found : undefined
}

export function list(value: unknown, key?: string): Json[] {
  const found = key === undefined ? value : field(value, key)
  return Array.isArray(found) ? found : []
}

export function objects(value: unknown, key?: string): JsonObject[] {
  return list(value, key).filter(isObject)
}

const apps: readonly string[] = ['sonarr', 'radarr', 'holyarrs']
const protocols: readonly string[] = ['usenet', 'torrent']

function isApp(value: string | undefined): value is App {
  return value !== undefined && apps.includes(value)
}

function isProtocol(value: string | undefined): value is Protocol {
  return value !== undefined && protocols.includes(value)
}

export function parseReleases(raw: unknown): Release[] {
  return objects(raw).map((item) => {
    const app = str(item, 'app')
    const protocol = str(item, 'protocol')
    const title = str(item, 'title')
    const reportedBytes = num(item, 'reportedBytes')
    if (!isApp(app) || !isProtocol(protocol) || title === undefined || !reportedBytes) {
      throw new Error(`bad release entry: ${JSON.stringify(item)}`)
    }
    return {
      app,
      protocol,
      title,
      reportedBytes,
      tvdbId: num(item, 'tvdbId'),
      season: num(item, 'season'),
      episode: num(item, 'episode'),
      tmdbId: num(item, 'tmdbId'),
      imdbId: str(item, 'imdbId'),
    }
  })
}

export async function loadReleases(path: string): Promise<Release[]> {
  return parseReleases(await Bun.file(path).json())
}
