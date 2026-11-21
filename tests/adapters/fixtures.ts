// Turns the recording proxy's log from the last end-to-end run (tests/adapters/.records) into
// fixtures under fixtures/adapters/<adapter>/recorded/<client>/: one file per distinct request a
// client sent, with Fangst's answer, which the client accepted (the run imported its releases).
// Credentials are replaced by the placeholders the adapter fixture tests use.
//
// bun run tests/adapters/fixtures.ts   (or: just record-adapter-fixtures)

import { mkdir, readdir, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { type Json, type JsonObject, isObject, list, num, objects, str } from './shared'

const here = import.meta.dir
const records = join(here, '.records')
const root = join(here, '..', '..', 'fixtures', 'adapters')
const FANGST_KEY = 'e2e-fangst-api-key-0000'
const FIXTURE_KEY = 'fixture-api-key-0000'
const FIXTURE_SESSION = 'fixture-session-0000'

/** Answers that depend on which jobs and categories exist; the replay test only checks these are
 * understood. */
const sabJobIndependent = new Set(['version', 'get_config', 'get_cats', 'fullstatus'])
const qbitJobIndependent = new Set([
  'auth/login',
  'app/version',
  'app/webapiVersion',
  'app/preferences',
])

function clean(text: string): string {
  return (
    text
      .replaceAll(FANGST_KEY, FIXTURE_KEY)
      .replace(/SID=[0-9A-Za-z_-]+/g, `SID=${FIXTURE_SESSION}`)
      // Sonarr and Radarr send the qBittorrent username and password as Basic auth on every call.
      .replace(/^Basic ([A-Za-z0-9+/=]+)$/, (_, b64: string) => {
        const decoded = clean(Buffer.from(b64, 'base64').toString('utf8'))
        return `Basic ${Buffer.from(decoded).toString('base64')}`
      })
  )
}

function pairs(value: Json | undefined, drop: string[] = []): [string, string][] {
  return list(value)
    .filter(Array.isArray)
    .flatMap((pair): [string, string][] => {
      const [k, v] = pair
      return typeof k === 'string' && typeof v === 'string' && !drop.includes(k)
        ? [[k, clean(v)]]
        : []
    })
}

function hex(base64: string): string {
  return Buffer.from(base64, 'base64').toString('hex')
}

interface Fixture {
  adapter: 'sabnzbd' | 'qbittorrent'
  slug: string
  size: number
  data: JsonObject
}

function convert(
  entry: JsonObject,
  client: string,
  version: string,
  date: string
): Fixture | undefined {
  const request = entry.request
  const response = entry.response
  if (!isObject(request) || !isObject(response)) return undefined
  const path = str(request, 'path') ?? ''
  const query = pairs(request.query)
  const status = num(response, 'status') ?? 0
  let adapter: Fixture['adapter']
  let endpoint: string
  let jobSpecific: boolean
  if (path.startsWith('/api/v2/')) {
    adapter = 'qbittorrent'
    endpoint = path.slice('/api/v2/'.length)
    jobSpecific = !qbitJobIndependent.has(endpoint)
  } else if (path === '/api') {
    adapter = 'sabnzbd'
    const mode = query.find(([k]) => k === 'mode')?.[1] ?? 'none'
    // `name` is an action for queue and history, and the NZB address for addurl.
    const name =
      mode === 'queue' || mode === 'history' ? query.find(([k]) => k === 'name')?.[1] : undefined
    endpoint = name === undefined ? mode : `${mode}-${name}`
    jobSpecific = !sabJobIndependent.has(mode)
  } else {
    return undefined
  }
  const sentBody = request.body
  const body: JsonObject = {}
  if (isObject(sentBody)) {
    const text = str(sentBody, 'text')
    const base64 = str(sentBody, 'base64')
    if (text !== undefined) body.body = clean(text)
    if (base64 !== undefined) body.body_hex = hex(base64)
  }
  const responseHeaders = pairs(response.headers)
  const contentType = responseHeaders.find(([k]) => k === 'content-type')?.[1] ?? ''
  const answer = isObject(response.body) ? clean(str(response.body, 'text') ?? '') : ''
  let parsed: Json = answer
  if (contentType.includes('json')) {
    try {
      parsed = JSON.parse(answer)
    } catch {
      parsed = answer
    }
  }
  const slug = `${endpoint.replaceAll('/', '-')}${status === 200 ? '' : `-${status}`}`
  return {
    adapter,
    slug,
    size: answer.length,
    data: {
      source: 'recorded',
      client,
      client_version: version,
      recorded: date,
      note: `The request as ${client} ${version} sent it to Fangst in the end-to-end test (tests/adapters), and Fangst's answer, which ${client} accepted. Credentials are placeholders.`,
      job_specific: jobSpecific,
      request: {
        method: str(request, 'method') ?? 'GET',
        path: adapter === 'qbittorrent' ? `/${endpoint}` : '/',
        query,
        headers: pairs(request.headers, ['content-length']),
        ...body,
      },
      response: {
        status,
        content_type: contentType.includes('json') ? 'json' : 'text',
        body: parsed,
        headers: responseHeaders.filter(([k]) => k === 'set-cookie' || k === 'content-type'),
      },
    },
  }
}

const results = objects(await Bun.file(join(records, 'results.json')).json())
const versions = new Map(results.map((r) => [str(r, 'app') ?? '', str(r, 'version') ?? 'unknown']))
const date = new Date().toISOString().slice(0, 10)
let written = 0
for (const file of await readdir(records)) {
  if (!file.endsWith('.jsonl')) continue
  const client = file.replace(/\.jsonl$/, '')
  const version = versions.get(client) ?? 'unknown'
  // Per adapter and slug, the answer with the most in it: a queue with slots beats an empty one.
  const chosen = new Map<string, Fixture>()
  for (const line of (await Bun.file(join(records, file)).text()).split('\n')) {
    if (line.trim() === '') continue
    const entry: unknown = JSON.parse(line)
    if (!isObject(entry)) continue
    const fixture = convert(entry, client, version, date)
    if (fixture === undefined) continue
    const key = `${fixture.adapter}/${fixture.slug}`
    const previous = chosen.get(key)
    if (previous === undefined || fixture.size > previous.size) chosen.set(key, fixture)
  }
  for (const adapter of ['sabnzbd', 'qbittorrent']) {
    await rm(join(root, adapter, 'recorded', client), { recursive: true, force: true })
  }
  for (const fixture of chosen.values()) {
    const dir = join(root, fixture.adapter, 'recorded', client)
    await mkdir(dir, { recursive: true })
    await Bun.write(join(dir, `${fixture.slug}.json`), `${JSON.stringify(fixture.data, null, 2)}\n`)
    written += 1
  }
}
const format = Bun.spawn(['bun', 'x', 'biome', 'format', '--write', root], { stdout: 'ignore' })
await format.exited
console.log(`wrote ${written} recorded fixtures under ${root}`)
