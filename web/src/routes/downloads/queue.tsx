import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { listJobs } from '../../api-dl'
import { ApiKeyForm } from '../../components/downloads/ApiKeyForm'
import { JobActions } from '../../components/downloads/JobActions'
import { JobStatusBadge } from '../../components/downloads/JobStatusBadge'
import { UnauthorizedError, dlKeys, formatBytes, formatTime, unwrap } from '../../lib/dl-api'

export default function Queue() {
  // No refetch interval: the event stream in DownloadsLayout patches this cache.
  const jobs = useQuery({
    queryKey: dlKeys.jobs,
    queryFn: async () => unwrap(await listJobs(), 'Queue'),
    retry: (count, error) => !(error instanceof UnauthorizedError) && count < 2,
  })

  if (jobs.error instanceof UnauthorizedError) {
    return <ApiKeyForm reason="The daemon rejected the saved API key. Enter the current one." />
  }

  return (
    <div>
      <div className="mb-6 flex items-baseline justify-between">
        <h1 className="text-2xl font-semibold text-foreground">Download queue</h1>
        {jobs.data && (
          <span className="text-sm text-muted">
            {jobs.data.length} {jobs.data.length === 1 ? 'job' : 'jobs'}
          </span>
        )}
      </div>

      {jobs.isPending && (
        <div className="flex justify-center py-16">
          <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
        </div>
      )}

      {jobs.error && !(jobs.error instanceof UnauthorizedError) && (
        <p className="rounded-md bg-red-500/10 px-4 py-3 text-sm text-red-400">
          {jobs.error.message}
        </p>
      )}

      {jobs.data?.length === 0 && (
        <div className="rounded-xl border border-dashed border-border py-16 text-center text-sm text-muted">
          The queue is empty. Jobs added through the API or an adapter show up here.
        </div>
      )}

      {jobs.data && jobs.data.length > 0 && (
        <ul className="divide-y divide-border overflow-hidden rounded-xl border border-border bg-card">
          {jobs.data.map((job) => (
            <li
              key={job.id}
              className="flex flex-col gap-3 px-5 py-4 sm:flex-row sm:items-center"
              data-testid="job-row"
            >
              <div className="min-w-0 flex-1">
                <Link
                  to="/downloads/$jobId"
                  params={{ jobId: String(job.id) }}
                  className="block truncate font-medium text-foreground hover:text-accent"
                >
                  {job.name}
                </Link>
                <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted">
                  <JobStatusBadge status={job.status} />
                  <span>{formatBytes(job.total_bytes)}</span>
                  <span className="capitalize">{job.kind}</span>
                  {job.category && <span>{job.category}</span>}
                  <span>Priority {job.priority}</span>
                  <span>Added {formatTime(job.added_at)}</span>
                </div>
                {job.status === 'failed' && job.error && (
                  <p className="mt-2 truncate text-xs text-red-400">{job.error}</p>
                )}
              </div>
              <JobActions job={job} />
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
