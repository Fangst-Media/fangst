// Runs the adapter end-to-end test on this machine: builds and starts the compose stack, runs the
// driver for one suite, saves the logs and recordings in tests/adapters/.records, and removes
// every container and volume afterwards, also on failure.
//
// bun run tests/adapters/run.ts [sabnzbd|qbittorrent|holyarrs|all] [--keep]

import { mkdir, rm } from 'node:fs/promises'
import { join } from 'node:path'

const here = import.meta.dir
const args = process.argv.slice(2)
const keep = args.includes('--keep')
const suite = args.find((a) => !a.startsWith('--')) ?? 'all'
const compose = ['docker', 'compose', '-f', join(here, 'compose.yml')]
const records = join(here, '.records')

async function run(cmd: string[], quiet = false): Promise<number> {
  console.log(`$ ${cmd.join(' ')}`)
  const child = Bun.spawn(cmd, { stdout: quiet ? 'ignore' : 'inherit', stderr: 'inherit' })
  return child.exited
}

await rm(records, { recursive: true, force: true })
await mkdir(records, { recursive: true })

let code = 1
try {
  // A leftover stack from an interrupted run would keep its owner account and libraries.
  await run([...compose, 'down', '--volumes', '--remove-orphans'], true)
  if ((await run([...compose, 'up', '--detach', '--build'])) !== 0) {
    throw new Error('the stack did not start')
  }
  code = await run([...compose, 'run', '--rm', 'driver', suite])
} catch (e) {
  console.error(String(e))
} finally {
  const logs = Bun.spawn([...compose, 'logs', '--no-color', '--timestamps'], { stdout: 'pipe' })
  await Bun.write(join(records, 'compose.log'), await new Response(logs.stdout).text())
  if (!keep) await run([...compose, 'down', '--volumes', '--remove-orphans'], true)
}
console.log(code === 0 ? 'adapter e2e: passed' : `adapter e2e: failed; logs in ${records}`)
process.exit(code)
