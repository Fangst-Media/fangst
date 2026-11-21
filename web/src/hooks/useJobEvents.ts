import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import type { JobDto } from '../api-dl'
import { dlKeys, eventsUrl, parseChange, upsertJob } from '../lib/dl-api'

export type EventsStatus = 'connecting' | 'live' | 'offline'

// Long enough not to hammer a daemon that is restarting, short enough that the queue
// catches up within a few seconds once it is back.
const RECONNECT_DELAY_MS = 3000

/**
 * Keeps the job caches in step with the daemon's event stream. Each (re)connect refetches
 * the list, because changes made while the socket was down are not replayed.
 */
export function useJobEvents(apiKey: string | null): EventsStatus {
  const queryClient = useQueryClient()
  const [status, setStatus] = useState<EventsStatus>('connecting')

  useEffect(() => {
    if (apiKey === null) {
      setStatus('offline')
      return
    }
    let socket: WebSocket | null = null
    let retry: ReturnType<typeof setTimeout> | undefined
    let stopped = false

    const connect = () => {
      setStatus('connecting')
      socket = new WebSocket(eventsUrl(apiKey))
      socket.onopen = () => {
        setStatus('live')
        void queryClient.invalidateQueries({ queryKey: ['dl'] })
      }
      socket.onmessage = (message: MessageEvent<unknown>) => {
        if (typeof message.data !== 'string') return
        const change = parseChange(message.data)
        if (change === null) return
        if (change.type === 'updated') {
          const { job } = change
          queryClient.setQueryData<JobDto[]>(dlKeys.jobs, (jobs) =>
            jobs === undefined ? jobs : upsertJob(jobs, job)
          )
          queryClient.setQueryData<JobDto>(dlKeys.job(job.id), job)
          void queryClient.invalidateQueries({ queryKey: dlKeys.timeline(job.id) })
        } else {
          const { id } = change
          queryClient.setQueryData<JobDto[]>(dlKeys.jobs, (jobs) =>
            jobs?.filter((job) => job.id !== id)
          )
          queryClient.removeQueries({ queryKey: dlKeys.job(id) })
          queryClient.removeQueries({ queryKey: dlKeys.timeline(id) })
        }
      }
      socket.onclose = () => {
        if (stopped) return
        setStatus('offline')
        retry = setTimeout(connect, RECONNECT_DELAY_MS)
      }
    }

    connect()
    return () => {
      stopped = true
      clearTimeout(retry)
      socket?.close()
    }
  }, [apiKey, queryClient])

  return status
}
