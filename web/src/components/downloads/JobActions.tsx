import { useState } from 'react'
import type { JobDto } from '../../api-dl'
import { canPause, canResume, useJobActions } from '../../hooks/useJobActions'

type JobActionsProps = {
  job: JobDto
  onDeleted?: () => void
}

const buttonClass =
  'rounded-md border border-border px-3 py-1.5 text-xs font-medium text-foreground/80 transition-colors hover:border-accent hover:text-foreground disabled:opacity-50'

export function JobActions({ job, onDeleted }: JobActionsProps) {
  const { pause, resume, remove } = useJobActions()
  const [confirming, setConfirming] = useState(false)
  const busy = pause.isPending || resume.isPending || remove.isPending
  const failure = pause.error ?? resume.error ?? remove.error

  return (
    <div className="flex flex-col items-end gap-1">
      <div className="flex items-center gap-2">
        {canPause(job) && (
          <button
            type="button"
            className={buttonClass}
            disabled={busy}
            onClick={() => pause.mutate(job.id)}
          >
            Pause
          </button>
        )}
        {canResume(job) && (
          <button
            type="button"
            className={buttonClass}
            disabled={busy}
            onClick={() => resume.mutate(job.id)}
          >
            {job.status === 'failed' ? 'Retry' : 'Resume'}
          </button>
        )}
        {confirming ? (
          <>
            <button
              type="button"
              className="rounded-md bg-red-500/80 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-red-500 disabled:opacity-50"
              disabled={busy}
              onClick={() =>
                remove.mutate(job.id, {
                  onSuccess: () => onDeleted?.(),
                  onSettled: () => setConfirming(false),
                })
              }
            >
              Confirm delete
            </button>
            <button type="button" className={buttonClass} onClick={() => setConfirming(false)}>
              Keep
            </button>
          </>
        ) : (
          <button
            type="button"
            className={`${buttonClass} hover:border-red-500 hover:text-red-400`}
            disabled={busy}
            onClick={() => setConfirming(true)}
          >
            Delete
          </button>
        )}
      </div>
      {failure !== null && <p className="text-xs text-red-400">{failure.message}</p>}
    </div>
  )
}
