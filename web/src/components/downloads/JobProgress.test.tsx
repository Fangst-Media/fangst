import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { makeJob } from '../../test/fixtures'
import { JobProgress } from './JobProgress'

const MIB = 1024 * 1024

describe('JobProgress', () => {
  it('shows the bar, bytes, rate and time left while downloading', () => {
    render(
      <JobProgress
        job={makeJob({
          status: 'downloading',
          total_bytes: 100 * MIB,
          downloaded_bytes: 25 * MIB,
          rate_bytes_per_sec: 5 * MIB,
          eta_seconds: 15,
        })}
      />
    )
    const bar = screen.getByRole('progressbar', { name: /progress/ })
    expect(bar).toHaveAttribute('value', '25')
    expect(screen.getByText('25%')).toBeInTheDocument()
    expect(screen.getByText('25.0 MB of 100.0 MB')).toBeInTheDocument()
    expect(screen.getByText('5.0 MB/s')).toBeInTheDocument()
    expect(screen.getByText('15s left')).toBeInTheDocument()
  })

  it('says the time left is unknown until there is a rate', () => {
    render(
      <JobProgress
        job={makeJob({
          status: 'downloading',
          total_bytes: 100,
          downloaded_bytes: 0,
          rate_bytes_per_sec: 0,
          eta_seconds: null,
        })}
      />
    )
    expect(screen.getByText('Time left unknown')).toBeInTheDocument()
    expect(screen.queryByText(/\/s$/)).not.toBeInTheDocument()
  })

  it('shows recorded bytes without rate or time for a paused job', () => {
    render(
      <JobProgress job={makeJob({ status: 'paused', total_bytes: 1000, downloaded_bytes: 500 })} />
    )
    expect(screen.getByRole('progressbar')).toHaveAttribute('value', '50')
    expect(screen.queryByText(/left/)).not.toBeInTheDocument()
    expect(screen.queryByText(/\/s$/)).not.toBeInTheDocument()
  })

  it('shows nothing for a completed job', () => {
    const { container } = render(
      <JobProgress
        job={makeJob({ status: 'completed', total_bytes: 1000, downloaded_bytes: 1000 })}
      />
    )
    expect(container).toBeEmptyDOMElement()
  })
})
