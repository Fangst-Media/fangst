import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { addJob } from '../../api-dl'
import type { JobDto } from '../../api-dl'
import { dlKeys, unwrap, upsertJob } from '../../lib/dl-api'

export function AddMagnetForm() {
  const queryClient = useQueryClient()
  const [link, setLink] = useState('')

  const add = useMutation({
    mutationFn: async (magnet: string) => unwrap(await addJob({ body: { magnet } }), 'Queue'),
    onSuccess: (job: JobDto) => {
      queryClient.setQueryData<JobDto[]>(dlKeys.jobs, (jobs) =>
        jobs === undefined ? jobs : upsertJob(jobs, job)
      )
      setLink('')
    },
  })

  return (
    <form
      className="mb-6 flex flex-col gap-2 sm:flex-row sm:items-end"
      onSubmit={(event) => {
        event.preventDefault()
        const magnet = link.trim()
        if (magnet !== '') add.mutate(magnet)
      }}
    >
      <div className="min-w-0 flex-1">
        <label className="block text-sm font-medium text-foreground" htmlFor="dl-magnet">
          Magnet link
        </label>
        <input
          id="dl-magnet"
          type="text"
          autoComplete="off"
          spellCheck={false}
          placeholder="magnet:?xt=urn:btih:..."
          value={link}
          onChange={(event) => setLink(event.target.value)}
          className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:border-accent focus:outline-none"
        />
        {add.error && (
          <p role="alert" className="mt-2 text-xs text-red-400">
            {add.error.message}
          </p>
        )}
      </div>
      <button
        type="submit"
        disabled={link.trim() === '' || add.isPending}
        className="rounded-md bg-accent px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-accent/90 disabled:opacity-50"
      >
        Add
      </button>
    </form>
  )
}
