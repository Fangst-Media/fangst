import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { JobStatus } from '../../api-dl'
import { JobStatusBadge } from './JobStatusBadge'

const labels: [JobStatus, string][] = [
  ['queued', 'Queued'],
  ['downloading', 'Downloading'],
  ['paused', 'Paused'],
  ['completed', 'Completed'],
  ['failed', 'Failed'],
]

describe('JobStatusBadge', () => {
  it.each(labels)('labels %s as %s', (status, label) => {
    render(<JobStatusBadge status={status} />)
    expect(screen.getByText(label)).toBeInTheDocument()
  })

  it('gives failed and completed different colours', () => {
    render(
      <>
        <JobStatusBadge status="failed" />
        <JobStatusBadge status="completed" />
      </>
    )
    expect(screen.getByText('Failed')).toHaveClass('text-red-400')
    expect(screen.getByText('Completed')).toHaveClass('text-green-400')
  })
})
