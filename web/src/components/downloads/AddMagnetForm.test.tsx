import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import type { JobDto } from '../../api-dl'
import { dlKeys, setApiKey } from '../../lib/dl-api'
import {
  jsonResponse,
  makeJob,
  makeQueryClient,
  queryWrapper,
  stubFetch,
} from '../../test/fixtures'
import { AddMagnetForm } from './AddMagnetForm'

const LINK = 'magnet:?xt=urn:btih:cab507494d02ebb1178b38f2e9d7be299c86b862&dn=Some.Release'

function renderForm() {
  const queryClient = makeQueryClient()
  queryClient.setQueryData<JobDto[]>(dlKeys.jobs, [makeJob({ id: 1 })])
  render(<AddMagnetForm />, { wrapper: queryWrapper(queryClient) })
  return queryClient
}

describe('AddMagnetForm', () => {
  beforeEach(() => {
    setApiKey('test-key-0123456789')
  })

  it('posts the link as JSON and puts the new job in the cached queue', async () => {
    const added = makeJob({ id: 2, kind: 'torrent', name: 'Some.Release' })
    const requests = stubFetch(() => jsonResponse(added, 201))
    const queryClient = renderForm()

    fireEvent.change(screen.getByLabelText('Magnet link'), { target: { value: LINK } })
    fireEvent.click(screen.getByRole('button', { name: 'Add' }))

    await waitFor(() => expect(queryClient.getQueryData<JobDto[]>(dlKeys.jobs)?.[0]?.id).toBe(2))
    expect(requests).toHaveLength(1)
    const request = requests[0]
    expect(request?.method).toBe('POST')
    expect(new URL(request?.url ?? '').pathname).toBe('/api/v1/jobs')
    expect(await request?.json()).toEqual({ magnet: LINK })
    expect(screen.getByLabelText('Magnet link')).toHaveValue('')
  })

  it('shows why the daemon refused the link and keeps it in the field', async () => {
    stubFetch(() => jsonResponse({ error: 'invalid torrent: no v1 (btih) info hash' }, 422))
    renderForm()

    fireEvent.change(screen.getByLabelText('Magnet link'), { target: { value: 'magnet:?dn=x' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add' }))

    expect(await screen.findByRole('alert')).toHaveTextContent('no v1 (btih) info hash')
    expect(screen.getByLabelText('Magnet link')).toHaveValue('magnet:?dn=x')
  })

  it('does not post an empty field', () => {
    const requests = stubFetch(() => jsonResponse({}, 500))
    renderForm()
    expect(screen.getByRole('button', { name: 'Add' })).toBeDisabled()
    expect(requests).toHaveLength(0)
  })
})
