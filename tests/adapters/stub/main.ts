// Three stubs in one process, so the compose file needs one Bun container for all of them
// (the third, a TMDB stand-in for HolyArrs on :9118, is described further down):
//
// - an indexer on :9117 that speaks just enough Newznab (/newznab/api) and Torznab
//   (/torznab/api) to hand each client its releases from releases.json, and serves the NZB and
//   torrent files the mocks container wrote;
// - a recording proxy per client (Sonarr :8081, Radarr :8082, HolyArrs :8083) in front of
//   Fangst. Every request and Fangst's answer go to /records/<client>.jsonl; the fixture writer
//   turns them into fixtures.

import { appendFile, mkdir } from 'node:fs/promises'
import { type App, type Json, type Protocol, type Release, isObject, loadReleases } from '../shared'

const env = (name: string, fallback: string): string => process.env[name] ?? fallback

const releases = await loadReleases(env('RELEASES_JSON', '/adapters/releases.json'))
const releasesDir = env('RELEASES_DIR', '/releases')
const recordsDir = env('RECORDS_DIR', '/records')
const upstream = env('UPSTREAM', 'http://fangst:7740')
const publicUrl = env('INDEXER_URL', 'http://indexer:9117')
const recorders: [App, number][] = [
  ['sonarr', 8081],
  ['radarr', 8082],
  ['holyarrs', 8083],
]

// ---------- indexer ----------

const caps = `<?xml version="1.0" encoding="UTF-8"?>
<caps>
  <server version="1.0" title="fangst-e2e-indexer"/>
  <limits max="100" default="100"/>
  <searching>
    <search available="yes" supportedParams="q"/>
    <tv-search available="yes" supportedParams="q,season,ep,tvdbid"/>
    <movie-search available="yes" supportedParams="q,imdbid,tmdbid"/>
  </searching>
  <categories>
    <category id="2000" name="Movies"><subcat id="2040" name="Movies/HD"/></category>
    <category id="5000" name="TV"><subcat id="5040" name="TV/HD"/></category>
  </categories>
</caps>`

function xml(text: string): string {
  return text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
}

function words(text: string): string[] {
  return text
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((w) => w.length > 0)
}

/** Which releases answer a search. No search terms at all is an RSS poll or a client's test,
 * which get every release of the app. */
function matches(release: Release, params: URLSearchParams): boolean {
  const tvdbid = params.get('tvdbid')
  const season = params.get('season')
  const ep = params.get('ep')
  const imdbid = params.get('imdbid')
  const tmdbid = params.get('tmdbid')
  const q = params.get('q')
  if (tvdbid !== null && String(release.tvdbId) !== tvdbid) return false
  if (season !== null && release.season !== Number(season.replace(/^s/i, ''))) return false
  if (ep !== null && release.episode !== Number(ep.replace(/^e/i, ''))) return false
  if (imdbid !== null && release.imdbId?.replace(/^tt/, '') !== imdbid.replace(/^tt/, '')) {
    return false
  }
  if (tmdbid !== null && String(release.tmdbId) !== tmdbid) return false
  if (q !== null && q.trim() !== '') {
    const title = new Set(words(release.title))
    if (!words(q).every((w) => title.has(w))) return false
  }
  return true
}

function item(release: Release, protocol: Protocol, apikey: string): string {
  const ext = protocol === 'usenet' ? 'nzb' : 'torrent'
  const type = protocol === 'usenet' ? 'application/x-nzb' : 'application/x-bittorrent'
  const link = `${publicUrl}/files/${encodeURIComponent(release.title)}.${ext}?apikey=${apikey}`
  const ns = protocol === 'usenet' ? 'newznab' : 'torznab'
  const tv = release.tvdbId !== undefined
  const category = tv ? 5040 : 2040
  const attrs: [string, string | number | undefined][] = [
    ['category', tv ? 5000 : 2000],
    ['category', category],
    ['size', release.reportedBytes],
    ['tvdbid', release.tvdbId],
    ['season', release.season === undefined ? undefined : `S${pad(release.season)}`],
    ['episode', release.episode === undefined ? undefined : `E${pad(release.episode)}`],
    ['imdb', release.imdbId?.replace(/^tt/, '')],
    ['tmdbid', release.tmdbId],
  ]
  if (protocol === 'torrent') {
    attrs.push(['seeders', 1], ['peers', 1], ['downloadvolumefactor', 1], ['uploadvolumefactor', 1])
  }
  // A day old: new enough for any retention, old enough that no client waits for it to settle.
  const published = new Date(Date.now() - 86_400_000).toUTCString()
  return `    <item>
      <title>${xml(release.title)}</title>
      <guid isPermaLink="false">${xml(`${protocol}-${release.title}`)}</guid>
      <link>${xml(link)}</link>
      <comments>${xml(`${publicUrl}/details/${release.title}`)}</comments>
      <pubDate>${published}</pubDate>
      <category>${category}</category>
      <size>${release.reportedBytes}</size>
      <enclosure url="${xml(link)}" length="${release.reportedBytes}" type="${type}"/>
${attrs
  .filter(([, value]) => value !== undefined)
  .map(([name, value]) => `      <${ns}:attr name="${name}" value="${xml(String(value))}"/>`)
  .join('\n')}
    </item>`
}

function pad(n: number): string {
  return String(n).padStart(2, '0')
}

function feed(protocol: Protocol, app: App, params: URLSearchParams, apikey: string): string {
  const found = releases.filter(
    (r) => r.app === app && r.protocol === protocol && matches(r, params)
  )
  const ns =
    protocol === 'usenet'
      ? 'xmlns:newznab="http://www.newznab.com/DTD/2010/feeds/attributes/"'
      : 'xmlns:torznab="http://torznab.com/schemas/2015/feed"'
  return `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom" ${ns}>
  <channel>
    <title>fangst-e2e-indexer</title>
    <description>Releases for the Fangst adapter end-to-end test</description>
    <link>${publicUrl}</link>
${found.map((r) => item(r, protocol, apikey)).join('\n')}
  </channel>
</rss>`
}

function xmlResponse(body: string, status = 200): Response {
  return new Response(body, { status, headers: { 'content-type': 'application/xml' } })
}

function isApp(key: string | null): key is App {
  return key === 'sonarr' || key === 'radarr' || key === 'holyarrs'
}

async function indexer(request: Request): Promise<Response> {
  const url = new URL(request.url)
  console.log(`indexer: ${request.method} ${url.pathname}${url.search}`)
  if (url.pathname.startsWith('/files/')) {
    const name = decodeURIComponent(url.pathname.slice('/files/'.length))
    if (name.includes('/') || name.includes('..')) return new Response('bad name', { status: 400 })
    const file = Bun.file(`${releasesDir}/${name}`)
    if (!(await file.exists())) return new Response('not found', { status: 404 })
    const type = name.endsWith('.nzb') ? 'application/x-nzb' : 'application/x-bittorrent'
    return new Response(file, {
      headers: { 'content-type': type, 'content-disposition': `attachment; filename="${name}"` },
    })
  }
  const protocol: Protocol | undefined =
    url.pathname === '/newznab/api'
      ? 'usenet'
      : url.pathname === '/torznab/api'
        ? 'torrent'
        : undefined
  if (protocol === undefined) return new Response('not found', { status: 404 })
  const t = url.searchParams.get('t')
  if (t === 'caps') return xmlResponse(caps)
  const apikey = url.searchParams.get('apikey')
  if (!isApp(apikey)) {
    return xmlResponse(
      '<?xml version="1.0" encoding="UTF-8"?><error code="100" description="Incorrect user credentials"/>'
    )
  }
  if (t === 'search' || t === 'tvsearch' || t === 'movie') {
    return xmlResponse(feed(protocol, apikey, url.searchParams, apikey))
  }
  return xmlResponse(
    `<?xml version="1.0" encoding="UTF-8"?><error code="202" description="No such function (${xml(t ?? '')})"/>`
  )
}

// ---------- recording proxy ----------

interface Body {
  text?: string
  base64?: string
}

const utf8 = new TextDecoder('utf-8', { fatal: true })

function body(bytes: Uint8Array): Body | undefined {
  if (bytes.length === 0) return undefined
  try {
    return { text: utf8.decode(bytes) }
  } catch {
    return { base64: Buffer.from(bytes).toString('base64') }
  }
}

function pairs(headers: Headers): [string, string][] {
  const out: [string, string][] = []
  headers.forEach((value, key) => {
    if (key !== 'set-cookie') out.push([key, value])
  })
  for (const cookie of headers.getSetCookie()) out.push(['set-cookie', cookie])
  return out
}

let sequence = 0
let writing: Promise<void> = Promise.resolve()

async function record(client: App, request: Request): Promise<Response> {
  const url = new URL(request.url)
  const sent = new Uint8Array(await request.arrayBuffer())
  const forward = new Headers(request.headers)
  // Fangst then answers uncompressed, so the recorded body is readable and can be passed on as is.
  forward.delete('accept-encoding')
  let answer: Response
  try {
    answer = await fetch(`${upstream}${url.pathname}${url.search}`, {
      method: request.method,
      headers: forward,
      body: sent.length > 0 ? sent : undefined,
      redirect: 'manual',
    })
  } catch (e) {
    console.error(`recorder ${client}: ${request.method} ${url.pathname}: ${e}`)
    return new Response('upstream unreachable', { status: 502 })
  }
  const received = new Uint8Array(await answer.arrayBuffer())
  const headers = new Headers(answer.headers)
  headers.delete('content-encoding')
  headers.delete('content-length')
  headers.delete('transfer-encoding')
  sequence += 1
  const entry = {
    seq: sequence,
    client,
    at: new Date().toISOString(),
    request: {
      method: request.method,
      path: url.pathname,
      query: [...url.searchParams.entries()],
      headers: pairs(request.headers),
      body: body(sent),
    },
    response: { status: answer.status, headers: pairs(answer.headers), body: body(received) },
  }
  // Concurrent appends from one process interleave long lines, so writes go one at a time.
  const line = `${JSON.stringify(entry)}\n`
  writing = writing
    .then(() => appendFile(`${recordsDir}/${client}.jsonl`, line))
    .catch((e) => console.error(`recorder ${client}: writing the log failed: ${e}`))
  await writing
  console.log(
    `recorder ${client}: ${request.method} ${url.pathname}${url.search} -> ${answer.status}`
  )
  return new Response(received, { status: answer.status, headers })
}

// ---------- TMDB stand-in ----------

// HolyArrs looks titles up on TMDB, through its own metadata server unless it has a key and a
// TMDB address of its own. This answers the few reads adding The Matrix and Breaking Bad needs,
// with the real ids, so the test does not depend on either service.
const movies: Record<string, Json> = {
  '603': {
    id: 603,
    title: 'The Matrix',
    original_title: 'The Matrix',
    release_date: '1999-03-31',
    original_language: 'en',
    overview: 'A hacker learns what the world really is.',
    runtime: 136,
    imdb_id: 'tt0133093',
    genres: [{ id: 28, name: 'Action' }],
    release_dates: {
      results: [
        {
          iso_3166_1: 'US',
          release_dates: [
            { type: 3, release_date: '1999-03-31T00:00:00.000Z', certification: 'R' },
          ],
        },
      ],
    },
  },
}
const breakingBad = {
  id: 1396,
  name: 'Breaking Bad',
  original_name: 'Breaking Bad',
  first_air_date: '2008-01-20',
  original_language: 'en',
  overview: 'A chemistry teacher turns to crime.',
  status: 'Ended',
  episode_run_time: [47],
  networks: [{ name: 'AMC' }],
  genres: [{ id: 18, name: 'Drama' }],
  external_ids: { imdb_id: 'tt0903747', tvdb_id: 81189 },
  seasons: [{ season_number: 1, name: 'Season 1', episode_count: 3, air_date: '2008-01-20' }],
}
const season1 = {
  season_number: 1,
  episodes: (
    [
      ['Pilot', '2008-01-20'],
      ["Cat's in the Bag...", '2008-01-27'],
      ["...And the Bag's in the River", '2008-02-10'],
    ] satisfies [string, string][]
  ).map(([name, air], i) => ({
    id: 62085 + i,
    season_number: 1,
    episode_number: i + 1,
    name,
    air_date: air,
    runtime: 47,
  })),
}

function json(value: Json, status = 200): Response {
  return Response.json(value, { status })
}

function tmdb(request: Request): Response {
  const url = new URL(request.url)
  const path = url.pathname.replace(/^\/3\//, '/')
  console.log(`tmdb: ${request.method} ${path}${url.search}`)
  const movie = path.match(/^\/movie\/(\d+)(\/release_dates)?$/)
  if (movie?.[1] !== undefined) {
    const found = movies[movie[1]]
    if (found === undefined) return json({ status_message: 'not found' }, 404)
    return json(movie[2] === undefined ? found : { id: Number(movie[1]), ...releaseDates(found) })
  }
  if (path === '/tv/1396') return json(breakingBad)
  if (path === '/tv/1396/season/1') return json(season1)
  if (path === '/tv/1396/external_ids') return json(breakingBad.external_ids)
  const find = path.match(/^\/find\/(\w+)$/)
  if (find !== null) {
    const id = find[1]
    return json({
      movie_results: id === 'tt0133093' ? [movies['603'] ?? null] : [],
      tv_results: id === 'tt0903747' || id === '81189' ? [breakingBad] : [],
    })
  }
  if (path === '/search/movie')
    return json({ page: 1, total_pages: 1, results: [movies['603'] ?? null] })
  if (path === '/search/tv') return json({ page: 1, total_pages: 1, results: [breakingBad] })
  if (path === '/genre/movie/list' || path === '/genre/tv/list') {
    return json({
      genres: [
        { id: 18, name: 'Drama' },
        { id: 28, name: 'Action' },
      ],
    })
  }
  if (path.endsWith('/changes')) return json({ results: [], page: 1, total_pages: 1 })
  return json({ status_message: 'not found' }, 404)
}

function releaseDates(movie: Json): { results: Json } {
  const dates = isObject(movie) ? movie.release_dates : undefined
  return { results: isObject(dates) ? (dates.results ?? []) : [] }
}

await mkdir(recordsDir, { recursive: true })
Bun.serve({ port: 9118, hostname: '0.0.0.0', fetch: tmdb })
Bun.serve({ port: 9117, hostname: '0.0.0.0', fetch: indexer })
for (const [client, port] of recorders) {
  Bun.serve({ port, hostname: '0.0.0.0', fetch: (request) => record(client, request) })
}
console.log(`stub: indexer on :9117, recorders on ${recorders.map(([, p]) => `:${p}`).join(' ')}`)
