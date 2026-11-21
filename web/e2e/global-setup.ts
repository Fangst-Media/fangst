import { type ChildProcess, spawn, spawnSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { createInterface } from 'node:readline'
import { fileURLToPath } from 'node:url'
import { type ViteDevServer, createServer } from 'vite'
import { ENV } from './env'

const webDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const repoRoot = path.resolve(webDir, '..')

// A cold debug build of the daemon is minutes; reading the bound address is a second.
const DAEMON_START_TIMEOUT_MS = 30_000

function cargo(args: string[], capture: boolean): string {
  const result = spawnSync('cargo', args, {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: ['ignore', capture ? 'pipe' : 'inherit', 'inherit'],
  })
  if (result.error !== undefined) throw result.error
  if (result.status !== 0) throw new Error(`cargo ${args.join(' ')} exited with ${result.status}`)
  return result.stdout ?? ''
}

/** Builds fangst-dl and returns the binary path, honouring CARGO_TARGET_DIR. */
function buildDaemon(): string {
  cargo(['build', '--quiet', '-p', 'fangst-dl'], false)
  const metadata: unknown = JSON.parse(
    cargo(['metadata', '--format-version', '1', '--no-deps'], true)
  )
  if (
    typeof metadata !== 'object' ||
    metadata === null ||
    !('target_directory' in metadata) ||
    typeof metadata.target_directory !== 'string'
  ) {
    throw new Error('cargo metadata did not report a target directory')
  }
  const exe = process.platform === 'win32' ? 'fangst-dl.exe' : 'fangst-dl'
  return path.join(metadata.target_directory, 'debug', exe)
}

/** Resolves with the address from the daemon's first stdout line, `listening on <addr>`. */
function waitForListening(daemon: ChildProcess, stderr: () => string): Promise<string> {
  return new Promise((resolve, reject) => {
    const { stdout } = daemon
    if (stdout === null) {
      reject(new Error('fangst-dl stdout is not piped'))
      return
    }
    const lines = createInterface({ input: stdout })
    const timer = setTimeout(() => {
      reject(new Error(`fangst-dl did not report its address in time\n${stderr()}`))
    }, DAEMON_START_TIMEOUT_MS)
    lines.once('line', (line) => {
      clearTimeout(timer)
      const match = /^listening on (\S+)$/.exec(line.trim())
      if (match?.[1] === undefined) {
        reject(new Error(`unexpected first line from fangst-dl: ${line}`))
      } else {
        resolve(match[1])
      }
    })
    daemon.once('exit', (code) => {
      clearTimeout(timer)
      reject(new Error(`fangst-dl exited with ${code} before listening\n${stderr()}`))
    })
  })
}

function stopDaemon(daemon: ChildProcess): Promise<void> {
  if (daemon.exitCode !== null || daemon.signalCode !== null) return Promise.resolve()
  return new Promise((resolve) => {
    daemon.once('exit', () => resolve())
    daemon.kill()
  })
}

export default async function globalSetup(): Promise<() => Promise<void>> {
  const apiKey = randomUUID()
  const dataDir = await mkdtemp(path.join(tmpdir(), 'fangst-e2e-'))
  let daemon: ChildProcess | undefined
  let vite: ViteDevServer | undefined

  // Backstop for a runner that exits without calling teardown, such as on a second Ctrl-C.
  const killOnExit = () => daemon?.kill()
  process.once('exit', killOnExit)

  const teardown = async () => {
    process.off('exit', killOnExit)
    await vite?.close()
    if (daemon !== undefined) await stopDaemon(daemon)
    // Windows can hold the SQLite file for a moment after the process exits.
    await rm(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 })
  }

  try {
    const binary = buildDaemon()
    let stderr = ''
    // No --host and no torrent engine: jobs stay queued, so the test never touches a usenet
    // server or a peer.
    const args = ['serve', '--data-dir', dataDir, '--listen', '127.0.0.1:0', '--no-torrent']
    daemon = spawn(binary, args, {
      cwd: repoRoot,
      env: { ...process.env, FANGST_API_KEY: apiKey },
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    daemon.stderr?.on('data', (chunk: Buffer) => {
      stderr += chunk.toString()
    })
    const daemonAddress = await waitForListening(daemon, () => stderr)
    const daemonUrl = `http://${daemonAddress}`

    // vite.config.ts reads this when createServer loads it.
    process.env.FANGST_DL_URL = daemonUrl
    vite = await createServer({
      root: webDir,
      configFile: path.join(webDir, 'vite.config.ts'),
      logLevel: 'warn',
      // Port 0 lets the smoke test run next to a `just web-dev` on 5173.
      server: { host: '127.0.0.1', port: 0, strictPort: true },
    })
    await vite.listen()
    const address = vite.httpServer?.address()
    if (address === null || address === undefined || typeof address === 'string') {
      throw new Error('Vite did not report a TCP address')
    }

    process.env[ENV.apiKey] = apiKey
    process.env[ENV.daemonUrl] = daemonUrl
    process.env[ENV.webUrl] = `http://127.0.0.1:${address.port}`
  } catch (error) {
    await teardown()
    throw error
  }

  return teardown
}
