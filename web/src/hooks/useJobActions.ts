import { useMutation, useQueryClient } from '@tanstack/react-query'
import { deleteJob, pauseJob, resumeJob } from '../api-dl'
import type { JobDto } from '../api-dl'
import { dlKeys, ensureOk, unwrap, upsertJob } from '../lib/dl-api'

/** Pause, resume and delete. The event stream confirms each change; the response just makes it instant. */
export function useJobActions() {
  const queryClient = useQueryClient()

  const applyJob = (job: JobDto) => {
    queryClient.setQueryData<JobDto[]>(dlKeys.jobs, (jobs) =>
      jobs === undefined ? jobs : upsertJob(jobs, job)
    )
    queryClient.setQueryData<JobDto>(dlKeys.job(job.id), job)
  }

  const pause = useMutation({
    mutationFn: async (id: number) => unwrap(await pauseJob({ path: { id } }), 'Job'),
    onSuccess: applyJob,
  })

  const resume = useMutation({
    mutationFn: async (id: number) => unwrap(await resumeJob({ path: { id } }), 'Job'),
    onSuccess: applyJob,
  })

  const remove = useMutation({
    // Downloaded files stay on disk; removing them is a separate, explicit choice.
    mutationFn: async (id: number) => {
      ensureOk(await deleteJob({ path: { id }, query: { delete_files: false } }), 'Job')
      return id
    },
    onSuccess: (id) => {
      queryClient.setQueryData<JobDto[]>(dlKeys.jobs, (jobs) =>
        jobs?.filter((job) => job.id !== id)
      )
    },
  })

  return { pause, resume, remove }
}

export function canPause(job: JobDto): boolean {
  return job.status === 'queued' || job.status === 'downloading'
}

// The daemon also resumes failed jobs, which puts them back in the queue as a retry.
export function canResume(job: JobDto): boolean {
  return job.status === 'paused' || job.status === 'failed'
}
