import type { Json } from '../shared'

export class HttpError extends Error {
  constructor(
    readonly status: number,
    readonly body: string,
    what: string
  ) {
    super(`${what}: HTTP ${status} ${body.slice(0, 2000)}`)
  }
}

/** Thrown from a waitFor check when waiting longer can't help. */
export class GiveUp extends Error {}

export interface Api {
  base: string
  headers: Record<string, string>
}

export async function call(
  api: Api,
  method: string,
  path: string,
  payload?: Json,
  extraHeaders: Record<string, string> = {}
): Promise<unknown> {
  const response = await fetch(`${api.base}${path}`, {
    method,
    headers: {
      accept: 'application/json',
      ...(payload === undefined ? {} : { 'content-type': 'application/json' }),
      ...api.headers,
      ...extraHeaders,
    },
    body: payload === undefined ? undefined : JSON.stringify(payload),
  })
  const text = await response.text()
  if (!response.ok) throw new HttpError(response.status, text, `${method} ${api.base}${path}`)
  if (text.trim() === '') return null
  try {
    const parsed: unknown = JSON.parse(text)
    return parsed
  } catch {
    return text
  }
}

export function log(scope: string, message: string): void {
  console.log(`[${new Date().toISOString().slice(11, 19)}] ${scope}: ${message}`)
}

export async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms))
}

/** Polls `check` until it returns a value, or fails with `what` once `timeoutMs` has passed. */
export async function waitFor<T>(
  what: string,
  timeoutMs: number,
  check: () => Promise<T | undefined>,
  intervalMs = 2000
): Promise<T> {
  const deadline = Date.now() + timeoutMs
  let last: unknown
  while (Date.now() < deadline) {
    try {
      const value = await check()
      if (value !== undefined) return value
      last = undefined
    } catch (e) {
      if (e instanceof GiveUp) throw e
      last = e
    }
    await sleep(intervalMs)
  }
  throw new Error(`timed out waiting for ${what}${last === undefined ? '' : `: ${String(last)}`}`)
}
