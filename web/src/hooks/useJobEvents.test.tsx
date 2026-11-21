import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ChangeDto, JobDto } from '../api-dl'
import { dlKeys } from '../lib/dl-api'
import { makeJob, makeQueryClient, queryWrapper } from '../test/fixtures'
import { useJobEvents } from './useJobEvents'

/** Stands in for the browser WebSocket so tests decide when the daemon opens, sends and drops. */
class FakeSocket {
  static instances: FakeSocket[] = []

  readonly url: string
  onopen: (() => void) | null = null
  onmessage: ((message: MessageEvent<unknown>) => void) | null = null
  onclose: (() => void) | null = null
  closedByClient = false

  constructor(url: string) {
    this.url = url
    FakeSocket.instances.push(this)
  }

  close(): void {
    this.closedByClient = true
    this.onclose?.()
  }

  open(): void {
    act(() => this.onopen?.())
  }

  send(change: ChangeDto | string): void {
    const data = typeof change === 'string' ? change : JSON.stringify(change)
    act(() => this.onmessage?.(new MessageEvent('message', { data })))
  }

  drop(): void {
    act(() => this.onclose?.())
  }
}

function latestSocket(): FakeSocket {
  const socket = FakeSocket.instances[FakeSocket.instances.length - 1]
  if (socket === undefined) throw new Error('no WebSocket was opened')
  return socket
}

const API_KEY = 'key with spaces&more'

function setup(initialJobs: JobDto[] = []) {
  const queryClient = makeQueryClient()
  queryClient.setQueryData<JobDto[]>(dlKeys.jobs, initialJobs)
  const hook = renderHook(({ apiKey }: { apiKey: string | null }) => useJobEvents(apiKey), {
    initialProps: { apiKey: API_KEY },
    wrapper: queryWrapper(queryClient),
  })
  return { queryClient, hook }
}

describe('useJobEvents', () => {
  beforeEach(() => {
    FakeSocket.instances = []
    vi.stubGlobal('WebSocket', FakeSocket)
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('connects with the key in the query string and goes live on open', () => {
    const { hook } = setup()
    expect(hook.result.current).toBe('connecting')

    const socket = latestSocket()
    const url = new URL(socket.url)
    expect(url.protocol).toBe('ws:')
    expect(url.pathname).toBe('/api/v1/events')
    expect(url.searchParams.get('api_key')).toBe(API_KEY)

    socket.open()
    expect(hook.result.current).toBe('live')
  })

  it('inserts a new job newest first and replaces an existing one', () => {
    const older = makeJob({ id: 1, name: 'older' })
    const { queryClient } = setup([older])
    const socket = latestSocket()
    socket.open()

    socket.send({ type: 'updated', job: makeJob({ id: 2, name: 'newer' }) })
    expect(queryClient.getQueryData<JobDto[]>(dlKeys.jobs)?.map((job) => job.id)).toEqual([2, 1])

    socket.send({ type: 'updated', job: { ...older, status: 'paused' } })
    const jobs = queryClient.getQueryData<JobDto[]>(dlKeys.jobs)
    expect(jobs?.map((job) => job.id)).toEqual([2, 1])
    expect(jobs?.[1]?.status).toBe('paused')
    expect(queryClient.getQueryData<JobDto>(dlKeys.job(1))?.status).toBe('paused')
  })

  it('removes a deleted job from the list and drops its detail caches', () => {
    const job = makeJob({ id: 4 })
    const { queryClient } = setup([job])
    queryClient.setQueryData(dlKeys.job(4), job)
    queryClient.setQueryData(dlKeys.timeline(4), [])
    const socket = latestSocket()
    socket.open()

    socket.send({ type: 'deleted', id: 4 })

    expect(queryClient.getQueryData<JobDto[]>(dlKeys.jobs)).toEqual([])
    expect(queryClient.getQueryState(dlKeys.job(4))).toBeUndefined()
    expect(queryClient.getQueryState(dlKeys.timeline(4))).toBeUndefined()
  })

  it('patches progress into the list and the job without refetching the timeline', () => {
    const running = makeJob({ id: 5, status: 'downloading', total_bytes: 1000 })
    const other = makeJob({ id: 6 })
    const { queryClient } = setup([other, running])
    const socket = latestSocket()
    // Opening refetches everything, so the caches are filled after it.
    socket.open()
    queryClient.setQueryData(dlKeys.job(5), running)
    queryClient.setQueryData(dlKeys.timeline(5), [])

    socket.send({
      type: 'progress',
      id: 5,
      downloaded_bytes: 250,
      rate_bytes_per_sec: 50,
      eta_seconds: 15,
    })

    const jobs = queryClient.getQueryData<JobDto[]>(dlKeys.jobs)
    expect(jobs?.find((job) => job.id === 5)).toMatchObject({
      downloaded_bytes: 250,
      rate_bytes_per_sec: 50,
      eta_seconds: 15,
    })
    expect(jobs?.find((job) => job.id === 6)).toBe(other)
    expect(queryClient.getQueryData<JobDto>(dlKeys.job(5))?.downloaded_bytes).toBe(250)
    expect(queryClient.getQueryState(dlKeys.timeline(5))?.isInvalidated).toBe(false)
  })

  it('ignores progress for a job it does not know', () => {
    const job = makeJob({ id: 1 })
    const { queryClient } = setup([job])
    const socket = latestSocket()
    socket.open()

    socket.send({
      type: 'progress',
      id: 99,
      downloaded_bytes: 1,
      rate_bytes_per_sec: 1,
      eta_seconds: null,
    })

    expect(queryClient.getQueryData<JobDto[]>(dlKeys.jobs)).toEqual([job])
    expect(queryClient.getQueryData(dlKeys.job(99))).toBeUndefined()
  })

  it('leaves the list unloaded when an update arrives before the first fetch', () => {
    const queryClient = makeQueryClient()
    renderHook(() => useJobEvents(API_KEY), { wrapper: queryWrapper(queryClient) })
    const socket = latestSocket()
    socket.open()

    socket.send({ type: 'updated', job: makeJob({ id: 9 }) })

    expect(queryClient.getQueryData(dlKeys.jobs)).toBeUndefined()
    expect(queryClient.getQueryData<JobDto>(dlKeys.job(9))?.id).toBe(9)
  })

  it('ignores frames that are not valid changes', () => {
    const job = makeJob({ id: 1 })
    const { queryClient } = setup([job])
    const socket = latestSocket()
    socket.open()

    socket.send('not json')
    socket.send(JSON.stringify({ type: 'updated', job: { id: 2 } }))
    socket.send(JSON.stringify({ type: 'renamed', id: 1 }))

    expect(queryClient.getQueryData<JobDto[]>(dlKeys.jobs)).toEqual([job])
  })

  it('goes offline when the socket drops and reconnects after a pause', () => {
    vi.useFakeTimers()
    const { hook } = setup()
    latestSocket().open()

    latestSocket().drop()
    expect(hook.result.current).toBe('offline')
    expect(FakeSocket.instances).toHaveLength(1)

    act(() => vi.advanceTimersByTime(3000))
    expect(FakeSocket.instances).toHaveLength(2)
    expect(hook.result.current).toBe('connecting')

    latestSocket().open()
    expect(hook.result.current).toBe('live')
  })

  it('closes the socket on unmount without reconnecting', () => {
    vi.useFakeTimers()
    const { hook } = setup()
    const socket = latestSocket()
    socket.open()

    hook.unmount()
    act(() => vi.advanceTimersByTime(10_000))

    expect(socket.closedByClient).toBe(true)
    expect(FakeSocket.instances).toHaveLength(1)
  })

  it('stays offline without a key and opens no socket', () => {
    const queryClient = makeQueryClient()
    const { result } = renderHook(() => useJobEvents(null), {
      wrapper: queryWrapper(queryClient),
    })

    expect(result.current).toBe('offline')
    expect(FakeSocket.instances).toHaveLength(0)
  })

  it('reconnects with the new key when the key changes', () => {
    const { hook } = setup()
    const first = latestSocket()

    hook.rerender({ apiKey: 'another-key-0123456789' })

    expect(first.closedByClient).toBe(true)
    expect(FakeSocket.instances).toHaveLength(2)
    expect(new URL(latestSocket().url).searchParams.get('api_key')).toBe('another-key-0123456789')
  })
})
