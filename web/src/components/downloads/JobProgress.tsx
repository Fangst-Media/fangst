import type { JobDto } from '../../api-dl'
import { formatBytes, formatEta, formatRate, percentDone } from '../../lib/dl-api'

/** Progress bar with bytes, and while downloading the rate and time left. */
export function JobProgress({ job }: { job: JobDto }) {
  const downloading = job.status === 'downloading'
  // A completed job is whole by definition, and an untouched queued one has nothing to show.
  if (job.status === 'completed' || (!downloading && job.downloaded_bytes <= 0)) return null

  const percent = percentDone(job)
  const eta = formatEta(job.eta_seconds)
  return (
    <div className="w-full" data-testid="job-progress">
      {/* Each engine styles the bar through its own pseudo-element. */}
      <progress
        aria-label={`${job.name} progress`}
        max={100}
        value={percent}
        className={`block h-1.5 w-full appearance-none overflow-hidden rounded-full bg-border [&::-webkit-progress-bar]:bg-border ${
          downloading
            ? '[&::-moz-progress-bar]:bg-accent [&::-webkit-progress-value]:bg-accent'
            : '[&::-moz-progress-bar]:bg-muted [&::-webkit-progress-value]:bg-muted'
        }`}
      />
      <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted">
        <span className="font-medium text-foreground">{percent}%</span>
        <span>
          {formatBytes(job.downloaded_bytes)} of {formatBytes(job.total_bytes)}
        </span>
        {downloading && job.rate_bytes_per_sec > 0 && (
          <span>{formatRate(job.rate_bytes_per_sec)}</span>
        )}
        {downloading && <span>{eta === null ? 'Time left unknown' : `${eta} left`}</span>}
      </div>
    </div>
  )
}
