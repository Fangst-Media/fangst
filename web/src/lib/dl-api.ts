import { useSyncExternalStore } from 'react'
import type { ChangeDto, JobDto, JobKind, JobStatus } from '../api-dl'
import { client } from '../api-dl/client.gen'

// The key lives in this browser only; the daemon has no login, so whoever can read
// localStorage on this origin can drive the queue.
const API_KEY_STORAGE = 'fangst.dl.apiKey'
const listeners = new Set<() => void>()

function readStoredKey(): string | null {
  try {
    const value = window.localStorage.getItem(API_KEY_STORAGE)
    return value === null || value === '' ? null : value
  } catch {
    return null
  }
}

let currentKey: string | null = readStoredKey()

export function setApiKey(key: string | null): void {
  try {
    if (key === null) window.localStorage.removeItem(API_KEY_STORAGE)
    else window.localStorage.setItem(API_KEY_STORAGE, key)
  } catch {
    // Private windows can refuse storage; the key still works for this tab.
  }
  currentKey = key
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function useApiKey(): string | null {
  return useSyncExternalStore(subscribe, () => currentKey)
}

// The generated client has no base URL, so requests go to this origin's /api/v1, which
// Vite proxies to the daemon in dev.
client.setConfig({ auth: () => currentKey ?? undefined })

export const dlKeys = {
  jobs: ['dl', 'jobs'] as const,
  job: (id: number) => ['dl', 'job', id] as const,
  timeline: (id: number) => ['dl', 'timeline', id] as const,
}

/** Thrown for 401 so pages can ask for a new key instead of showing a generic error. */
export class UnauthorizedError extends Error {
  constructor() {
    super('The download daemon rejected the API key.')
    this.name = 'UnauthorizedError'
  }
}

export class NotFoundError extends Error {
  constructor(what: string) {
    super(`${what} not found.`)
    this.name = 'NotFoundError'
  }
}

type ApiResult<T> = {
  data?: T
  error?: unknown
  response?: Response
}

/** Unwraps a generated SDK result, mapping status codes to errors the pages understand. */
export function unwrap<T>(result: ApiResult<T>, what: string): T {
  const status = result.response?.status
  if (status === 401) throw new UnauthorizedError()
  if (status === 404) throw new NotFoundError(what)
  if (result.error !== undefined || result.data === undefined) {
    throw new Error(errorMessage(result.error, status))
  }
  return result.data
}

/** Like `unwrap` for calls whose success has no body, such as 204 on delete. */
export function ensureOk(result: ApiResult<unknown>, what: string): void {
  const status = result.response?.status
  if (status === 401) throw new UnauthorizedError()
  if (status === 404) throw new NotFoundError(what)
  if (result.error !== undefined) throw new Error(errorMessage(result.error, status))
}

function errorMessage(error: unknown, status: number | undefined): string {
  if (isRecord(error) && typeof error.error === 'string') return error.error
  return status === undefined
    ? 'Could not reach the download daemon.'
    : `Request failed (${status})`
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

const JOB_STATUSES: readonly JobStatus[] = [
  'queued',
  'downloading',
  'paused',
  'completed',
  'failed',
]
const JOB_KINDS: readonly JobKind[] = ['usenet', 'torrent']

function isJobStatus(value: unknown): value is JobStatus {
  return JOB_STATUSES.some((status) => status === value)
}

function isJobKind(value: unknown): value is JobKind {
  return JOB_KINDS.some((kind) => kind === value)
}

function isOptionalString(value: unknown): boolean {
  return value === undefined || value === null || typeof value === 'string'
}

function isOptionalNumber(value: unknown): boolean {
  return value === undefined || value === null || typeof value === 'number'
}

function isJobDto(value: unknown): value is JobDto {
  return (
    isRecord(value) &&
    typeof value.id === 'number' &&
    typeof value.name === 'string' &&
    typeof value.priority === 'number' &&
    typeof value.total_bytes === 'number' &&
    typeof value.downloaded_bytes === 'number' &&
    typeof value.rate_bytes_per_sec === 'number' &&
    isOptionalNumber(value.eta_seconds) &&
    typeof value.added_at === 'number' &&
    isJobKind(value.kind) &&
    isJobStatus(value.status) &&
    isOptionalString(value.category) &&
    isOptionalString(value.error) &&
    isOptionalString(value.output_path) &&
    isOptionalNumber(value.finished_at)
  )
}

/** WebSocket frames arrive as untyped JSON, so they are checked against the generated shape. */
export function parseChange(raw: string): ChangeDto | null {
  let value: unknown
  try {
    value = JSON.parse(raw)
  } catch {
    return null
  }
  if (!isRecord(value)) return null
  if (value.type === 'updated' && isJobDto(value.job)) {
    return { type: 'updated', job: value.job }
  }
  if (value.type === 'deleted' && typeof value.id === 'number') {
    return { type: 'deleted', id: value.id }
  }
  if (
    value.type === 'progress' &&
    typeof value.id === 'number' &&
    typeof value.downloaded_bytes === 'number' &&
    typeof value.rate_bytes_per_sec === 'number' &&
    (value.eta_seconds === undefined ||
      value.eta_seconds === null ||
      typeof value.eta_seconds === 'number')
  ) {
    return {
      type: 'progress',
      id: value.id,
      downloaded_bytes: value.downloaded_bytes,
      rate_bytes_per_sec: value.rate_bytes_per_sec,
      eta_seconds: value.eta_seconds,
    }
  }
  return null
}

export type ProgressChange = Extract<ChangeDto, { type: 'progress' }>

export function applyProgress(job: JobDto, progress: ProgressChange): JobDto {
  return {
    ...job,
    downloaded_bytes: progress.downloaded_bytes,
    rate_bytes_per_sec: progress.rate_bytes_per_sec,
    eta_seconds: progress.eta_seconds,
  }
}

export function percentDone(job: JobDto): number {
  if (job.total_bytes <= 0) return 0
  return Math.min(100, Math.floor((job.downloaded_bytes / job.total_bytes) * 100))
}

export function eventsUrl(apiKey: string): string {
  const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws'
  return `${scheme}://${window.location.host}/api/v1/events?api_key=${encodeURIComponent(apiKey)}`
}

/** Matches the daemon's list order, newest first, so live inserts land where a reload would put them. */
export function upsertJob(jobs: readonly JobDto[], job: JobDto): JobDto[] {
  const rest = jobs.filter((existing) => existing.id !== job.id)
  return [...rest, job].sort((a, b) => b.id - a.id)
}

const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB']

export function formatBytes(bytes: number): string {
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${BYTE_UNITS[unit]}`
}

export function formatRate(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`
}

function twoDigits(n: number): string {
  return String(n).padStart(2, '0')
}

/** The two largest units, which is as precise as an estimate from a moving average gets. */
export function formatEta(seconds: number | null | undefined): string | null {
  if (seconds === null || seconds === undefined) return null
  const days = Math.floor(seconds / 86_400)
  const hours = Math.floor((seconds % 86_400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const secs = Math.floor(seconds % 60)
  if (days > 0) return `${days}d ${twoDigits(hours)}h`
  if (hours > 0) return `${hours}h ${twoDigits(minutes)}m`
  if (minutes > 0) return `${minutes}m ${twoDigits(secs)}s`
  return `${secs}s`
}

export function formatTime(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString()
}
