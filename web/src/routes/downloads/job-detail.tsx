import { useQuery } from '@tanstack/react-query'
import { Link, useNavigate } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { getJob, jobTimeline } from '../../api-dl'
import { ApiKeyForm } from '../../components/downloads/ApiKeyForm'
import { JobActions } from '../../components/downloads/JobActions'
import { JobProgress } from '../../components/downloads/JobProgress'
import { JobStatusBadge } from '../../components/downloads/JobStatusBadge'
import {
  NotFoundError,
  UnauthorizedError,
  dlKeys,
  formatBytes,
  formatTime,
  unwrap,
} from '../../lib/dl-api'

function retryUnlessFinal(count: number, error: Error): boolean {
  return !(error instanceof UnauthorizedError || error instanceof NotFoundError) && count < 2
}

export default function JobDetail({ jobId }: { jobId: number }) {
  const navigate = useNavigate()
  const job = useQuery({
    queryKey: dlKeys.job(jobId),
    queryFn: async () => unwrap(await getJob({ path: { id: jobId } }), 'Job'),
    retry: retryUnlessFinal,
  })
  // The event stream invalidates this on every change to the job, which is when new events land.
  const timeline = useQuery({
    queryKey: dlKeys.timeline(jobId),
    queryFn: async () => unwrap(await jobTimeline({ path: { id: jobId } }), 'Job'),
    retry: retryUnlessFinal,
  })

  const back = (
    <Link to="/downloads" className="text-sm text-muted hover:text-foreground">
      &larr; Queue
    </Link>
  )

  if (job.error instanceof UnauthorizedError) {
    return <ApiKeyForm reason="The daemon rejected the saved API key. Enter the current one." />
  }
  if (job.error instanceof NotFoundError) {
    return (
      <div className="space-y-4">
        {back}
        <p className="text-muted">Job {jobId} does not exist, or it was deleted.</p>
      </div>
    )
  }
  if (job.error) {
    return (
      <div className="space-y-4">
        {back}
        <p className="rounded-md bg-red-500/10 px-4 py-3 text-sm text-red-400">
          {job.error.message}
        </p>
      </div>
    )
  }
  if (job.isPending) {
    return (
      <div className="flex justify-center py-16">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
      </div>
    )
  }

  const data = job.data
  return (
    <div className="space-y-6">
      {back}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <h1 className="break-words text-2xl font-semibold text-foreground">{data.name}</h1>
          <div className="mt-2">
            <JobStatusBadge status={data.status} />
          </div>
        </div>
        <JobActions job={data} onDeleted={() => void navigate({ to: '/downloads' })} />
      </div>

      <JobProgress job={data} />

      {data.status === 'failed' && (
        <div
          className="rounded-xl border border-red-500/30 bg-red-500/10 px-5 py-4"
          data-testid="failure"
        >
          <h2 className="text-sm font-semibold text-red-400">Why it failed</h2>
          <p className="mt-1 whitespace-pre-wrap break-words text-sm text-red-300">
            {data.error ?? 'The daemon recorded no reason.'}
          </p>
        </div>
      )}

      <dl className="grid grid-cols-1 gap-x-8 gap-y-3 rounded-xl border border-border bg-card px-5 py-4 text-sm sm:grid-cols-2">
        <Field label="Job ID">{data.id}</Field>
        <Field label="Kind">
          <span className="capitalize">{data.kind}</span>
        </Field>
        <Field label="Size">{formatBytes(data.total_bytes)}</Field>
        <Field label="Priority">{data.priority}</Field>
        <Field label="Category">{data.category ?? 'None'}</Field>
        <Field label="Added">{formatTime(data.added_at)}</Field>
        <Field label="Finished">
          {data.finished_at == null ? 'Not yet' : formatTime(data.finished_at)}
        </Field>
        <Field label="Output">
          <span className="break-all font-mono text-xs">{data.output_path ?? 'Not yet'}</span>
        </Field>
      </dl>

      <section>
        <h2 className="mb-3 text-lg font-semibold text-foreground">Timeline</h2>
        {timeline.isPending && <p className="text-sm text-muted">Loading timeline...</p>}
        {timeline.error && <p className="text-sm text-red-400">{timeline.error.message}</p>}
        {timeline.data?.length === 0 && <p className="text-sm text-muted">No events yet.</p>}
        {timeline.data && timeline.data.length > 0 && (
          <ol className="relative ml-2 border-l border-border" data-testid="timeline">
            {timeline.data.map((event, index) => (
              <li key={`${event.at}-${index}`} className="mb-5 ml-5 last:mb-0">
                <span className="absolute -left-1.5 mt-1.5 h-3 w-3 rounded-full border-2 border-background bg-accent" />
                <div className="flex flex-wrap items-baseline gap-x-3">
                  <span className="text-sm font-medium capitalize text-foreground">
                    {event.kind}
                  </span>
                  <time
                    className="text-xs text-muted"
                    dateTime={new Date(event.at * 1000).toISOString()}
                  >
                    {formatTime(event.at)}
                  </time>
                </div>
                <p className="mt-0.5 text-sm text-foreground/70">{event.message}</p>
              </li>
            ))}
          </ol>
        )}
      </section>
    </div>
  )
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <dt className="text-xs uppercase tracking-wide text-muted">{label}</dt>
      <dd className="mt-0.5 text-foreground">{children}</dd>
    </div>
  )
}
