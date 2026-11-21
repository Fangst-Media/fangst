import { describe, expect, it } from 'vitest'
import { makeJob } from '../test/fixtures'
import { applyProgress, formatEta, formatRate, parseChange, percentDone } from './dl-api'

describe('parseChange', () => {
  it('reads a progress message', () => {
    const raw = JSON.stringify({
      type: 'progress',
      id: 3,
      downloaded_bytes: 500,
      rate_bytes_per_sec: 100,
      eta_seconds: 5,
    })
    expect(parseChange(raw)).toEqual({
      type: 'progress',
      id: 3,
      downloaded_bytes: 500,
      rate_bytes_per_sec: 100,
      eta_seconds: 5,
    })
  })

  it('accepts an unknown time left', () => {
    const raw = JSON.stringify({
      type: 'progress',
      id: 3,
      downloaded_bytes: 0,
      rate_bytes_per_sec: 0,
      eta_seconds: null,
    })
    expect(parseChange(raw)).toMatchObject({ type: 'progress', eta_seconds: null })
  })

  it('refuses a progress message without numbers', () => {
    expect(parseChange(JSON.stringify({ type: 'progress', id: 3 }))).toBeNull()
  })

  it('refuses a job without byte counts', () => {
    const { downloaded_bytes: _, ...job } = makeJob()
    expect(parseChange(JSON.stringify({ type: 'updated', job }))).toBeNull()
  })
})

describe('applyProgress', () => {
  it('copies the live numbers onto the job', () => {
    const job = makeJob({ id: 3, status: 'downloading', total_bytes: 1000 })
    const updated = applyProgress(job, {
      type: 'progress',
      id: 3,
      downloaded_bytes: 400,
      rate_bytes_per_sec: 200,
      eta_seconds: 3,
    })
    expect(updated).toMatchObject({
      downloaded_bytes: 400,
      rate_bytes_per_sec: 200,
      eta_seconds: 3,
    })
    expect(job.downloaded_bytes).toBe(0)
  })
})

describe('percentDone', () => {
  it('is a whole percentage capped at 100', () => {
    expect(percentDone(makeJob({ total_bytes: 0, downloaded_bytes: 0 }))).toBe(0)
    expect(percentDone(makeJob({ total_bytes: 1000, downloaded_bytes: 455 }))).toBe(45)
    expect(percentDone(makeJob({ total_bytes: 1000, downloaded_bytes: 2000 }))).toBe(100)
  })
})

describe('formatRate', () => {
  it('uses binary units per second', () => {
    expect(formatRate(0)).toBe('0 B/s')
    expect(formatRate(1536)).toBe('1.5 KB/s')
    expect(formatRate(5 * 1024 * 1024)).toBe('5.0 MB/s')
  })
})

describe('formatEta', () => {
  it('shows the two largest units', () => {
    expect(formatEta(null)).toBeNull()
    expect(formatEta(undefined)).toBeNull()
    expect(formatEta(0)).toBe('0s')
    expect(formatEta(45)).toBe('45s')
    expect(formatEta(185)).toBe('3m 05s')
    expect(formatEta(3720)).toBe('1h 02m')
    expect(formatEta(90_000)).toBe('1d 01h')
  })
})
