import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { JobDto } from '../../api-dl'
import { dlKeys, setApiKey } from '../../lib/dl-api'
import {
  jsonResponse,
  makeJob,
  makeQueryClient,
  queryWrapper,
  stubFetch,
} from '../../test/fixtures'
import { JobActions } from './JobActions'

const API_KEY = 'test-key-0123456789'

function renderActions(job: JobDto, onDeleted?: () => void) {
  const queryClient = makeQueryClient()
  queryClient.setQueryData<JobDto[]>(dlKeys.jobs, [job])
  render(<JobActions job={job} onDeleted={onDeleted} />, { wrapper: queryWrapper(queryClient) })
  return queryClient
}

describe('JobActions', () => {
  beforeEach(() => {
    setApiKey(API_KEY)
  })

  it('offers pause for a queued job and no resume', () => {
    renderActions(makeJob({ status: 'queued' }))
    expect(screen.getByRole('button', { name: 'Pause' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Resume' })).not.toBeInTheDocument()
  })

  it('offers resume for a paused job and no pause', () => {
    renderActions(makeJob({ status: 'paused' }))
    expect(screen.getByRole('button', { name: 'Resume' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Pause' })).not.toBeInTheDocument()
  })

  it('calls resume on a failed job a retry', () => {
    renderActions(makeJob({ status: 'failed' }))
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument()
  })

  it('offers neither pause nor resume for a completed job', () => {
    renderActions(makeJob({ status: 'completed' }))
    expect(screen.queryByRole('button', { name: 'Pause' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Resume|Retry/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Delete' })).toBeInTheDocument()
  })

  it('pauses through the API with the saved key and updates the cached queue', async () => {
    const job = makeJob({ id: 7, status: 'queued' })
    const requests = stubFetch(() => jsonResponse({ ...job, status: 'paused' }))
    const queryClient = renderActions(job)

    fireEvent.click(screen.getByRole('button', { name: 'Pause' }))

    await waitFor(() =>
      expect(queryClient.getQueryData<JobDto[]>(dlKeys.jobs)?.[0]?.status).toBe('paused')
    )
    expect(requests).toHaveLength(1)
    const [request] = requests
    expect(request?.method).toBe('POST')
    expect(new URL(request?.url ?? '').pathname).toBe('/api/v1/jobs/7/pause')
    expect(request?.headers.get('X-Api-Key')).toBe(API_KEY)
    expect(queryClient.getQueryData<JobDto>(dlKeys.job(7))?.status).toBe('paused')
  })

  it('asks before deleting and keeps the files on disk', async () => {
    const job = makeJob({ id: 3 })
    const requests = stubFetch(() => new Response(null, { status: 204 }))
    const onDeleted = vi.fn()
    const queryClient = renderActions(job, onDeleted)

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }))
    expect(requests).toHaveLength(0)
    fireEvent.click(screen.getByRole('button', { name: 'Confirm delete' }))

    await waitFor(() => expect(onDeleted).toHaveBeenCalledOnce())
    const [request] = requests
    expect(request?.method).toBe('DELETE')
    const url = new URL(request?.url ?? '')
    expect(url.pathname).toBe('/api/v1/jobs/3')
    expect(url.searchParams.get('delete_files')).toBe('false')
    expect(queryClient.getQueryData<JobDto[]>(dlKeys.jobs)).toEqual([])
  })

  it('backs out of a delete with Keep', () => {
    const requests = stubFetch(() => new Response(null, { status: 204 }))
    renderActions(makeJob())

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }))
    fireEvent.click(screen.getByRole('button', { name: 'Keep' }))

    expect(screen.getByRole('button', { name: 'Delete' })).toBeInTheDocument()
    expect(requests).toHaveLength(0)
  })

  it('shows the daemon error message when an action fails', async () => {
    stubFetch(() => jsonResponse({ error: 'job is already finished' }, 409))
    renderActions(makeJob({ status: 'queued' }))

    fireEvent.click(screen.getByRole('button', { name: 'Pause' }))

    expect(await screen.findByText('job is already finished')).toBeInTheDocument()
  })

  it('says the key was rejected on 401', async () => {
    stubFetch(() => jsonResponse({ error: 'unauthorized' }, 401))
    renderActions(makeJob({ status: 'queued' }))

    fireEvent.click(screen.getByRole('button', { name: 'Pause' }))

    expect(await screen.findByText('The download daemon rejected the API key.')).toBeInTheDocument()
  })
})
