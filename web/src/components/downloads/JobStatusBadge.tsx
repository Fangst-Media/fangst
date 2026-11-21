import type { JobStatus } from '../../api-dl'

const statusStyle: Record<JobStatus, { label: string; className: string }> = {
  queued: { label: 'Queued', className: 'bg-blue-500/10 text-blue-400' },
  downloading: { label: 'Downloading', className: 'bg-accent/15 text-accent' },
  paused: { label: 'Paused', className: 'bg-yellow-500/10 text-yellow-400' },
  completed: { label: 'Completed', className: 'bg-green-500/10 text-green-400' },
  failed: { label: 'Failed', className: 'bg-red-500/10 text-red-400' },
}

export function JobStatusBadge({ status }: { status: JobStatus }) {
  const style = statusStyle[status]
  return (
    <span
      className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium ${style.className}`}
    >
      {style.label}
    </span>
  )
}
