// Configures each app through its own API, runs its connection tests, grabs one release per
// protocol and waits for the import. Usage: bun run driver/main.ts [sabnzbd|qbittorrent|holyarrs|all]

import { type App, type Protocol, type Release, loadReleases } from '../shared'
import {
  type ArrApp,
  type ArrConfig,
  type Outcome,
  addTargets,
  configure,
  grabAndImport,
  version,
  waitUntilUp,
} from './arr'
import { call, log, waitFor } from './http'

const suites: Record<string, { apps: App[]; protocols: Protocol[] }> = {
  sabnzbd: { apps: ['sonarr', 'radarr'], protocols: ['usenet'] },
  qbittorrent: { apps: ['sonarr', 'radarr'], protocols: ['torrent'] },
  holyarrs: { apps: ['holyarrs'], protocols: ['usenet', 'torrent'] },
  all: { apps: ['sonarr', 'radarr', 'holyarrs'], protocols: ['usenet', 'torrent'] },
}

function required(name: string): string {
  const value = process.env[name]
  if (value === undefined || value === '') throw new Error(`${name} is not set`)
  return value
}

const suiteName = process.argv[2] ?? 'all'
const suite = suites[suiteName]
if (suite === undefined) {
  console.error(`unknown suite ${suiteName}; one of ${Object.keys(suites).join(', ')}`)
  process.exit(2)
}

const fangstApiKey = required('FANGST_API_KEY')
const releases = await loadReleases('/adapters/releases.json')

await waitFor('Fangst to start', 5 * 60_000, async () => {
  await call({ base: 'http://fangst:7740', headers: {} }, 'GET', '/api/v1/health')
  return true
})
log('fangst', 'up')

const arrApps: Record<ArrApp, { url: string; key: string; recorderPort: number }> = {
  sonarr: { url: 'http://sonarr:8989', key: 'SONARR_API_KEY', recorderPort: 8081 },
  radarr: { url: 'http://radarr:7878', key: 'RADARR_API_KEY', recorderPort: 8082 },
}

async function runArr(
  app: ArrApp,
  protocols: Protocol[]
): Promise<{ outcomes: Outcome[]; version: string }> {
  const info = arrApps[app]
  const cfg: ArrConfig = {
    app,
    api: { base: info.url, headers: { 'x-api-key': required(info.key) } },
    recorderPort: info.recorderPort,
    fangstApiKey,
    protocols,
  }
  await waitUntilUp(cfg)
  await configure(cfg)
  const mine: Release[] = releases.filter((r) => r.app === app && protocols.includes(r.protocol))
  const targets = await addTargets(cfg, mine)
  const outcomes: Outcome[] = []
  // One at a time, so a failure names the release that caused it.
  for (const target of targets) outcomes.push(await grabAndImport(cfg, target))
  return { outcomes, version: await version(cfg) }
}

const runs: Promise<{ app: App; outcomes: Outcome[]; version: string }>[] = suite.apps.map(
  async (app) => {
    try {
      if (app === 'holyarrs') {
        const { runHolyarrs } = await import('./holyarrs')
        return { app, ...(await runHolyarrs(releases, suite.protocols, fangstApiKey)) }
      }
      return { app, ...(await runArr(app, suite.protocols)) }
    } catch (e) {
      log(app, `failed: ${e instanceof Error ? e.message : String(e)}`)
      const failure: Outcome = {
        app,
        release: '-',
        protocol: 'usenet',
        ok: false,
        detail: String(e),
      }
      return {
        app,
        version: 'unknown',
        outcomes: [failure],
      }
    }
  }
)

const results = await Promise.all(runs)
// The fixture writer names the client version in every recorded fixture.
await Bun.write(
  '/records/results.json',
  JSON.stringify(
    results.map(({ app, version: v, outcomes }) => ({ app, version: v, outcomes })),
    null,
    2
  )
)
console.log('\nResults')
let failed = false
for (const { app, version: v, outcomes } of results) {
  for (const o of outcomes) {
    failed ||= !o.ok
    console.log(
      `${o.ok ? 'PASS' : 'FAIL'}  ${app} ${v}  ${o.protocol.padEnd(7)}  ${o.release}  ${o.detail}`
    )
  }
}
process.exit(failed ? 1 : 0)
