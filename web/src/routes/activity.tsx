import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiFetch, apiPost } from '../lib/api-client'

type QueueItem = {
  id: string
  filename: string
  status: string
  sizeMB: number
  remainingMB: number
  percentage: number
  timeLeft: string
  category: string
  downloader: string
  downloaderId: string
}

type HistoryItem = {
  id: string
  name: string
  status: string
  bytes: number
  size: string
  category: string
  completedAt: string
  downloadTime: number
  failMessage?: string
  downloader: string
  downloaderId: string
}

type ActivityStats = {
  status: string
  speed: string
  sizeLeft: string
  timeLeft: string
  daySize: string
  weekSize: string
  monthSize: string
  totalSize: string
}

type ActivityResponse = {
  queue: QueueItem[]
  history: HistoryItem[]
  stats: ActivityStats | null
  errors?: { downloader: string; error: string }[]
}

async function fetchActivity(): Promise<ActivityResponse> {
  return apiFetch('/api/activity')
}

async function pauseDownloads(): Promise<void> {
  return apiPost('/api/activity/pause', {})
}

async function resumeDownloads(): Promise<void> {
  return apiPost('/api/activity/resume', {})
}

async function deleteItem(id: string, downloaderId: string): Promise<void> {
  return apiFetch(`/api/activity/${id}?downloaderId=${downloaderId}`, { method: 'DELETE' })
}

function formatSpeed(speed: string): string {
  if (!speed || speed === '0') return 'Idle'
  return speed
}

function formatTimeLeft(timeLeft: string): string {
  if (!timeLeft || timeLeft === '0:00:00') return '-'
  return timeLeft
}

function formatDownloadTime(seconds: number): string {
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
  const hours = Math.floor(seconds / 3600)
  const mins = Math.floor((seconds % 3600) / 60)
  return `${hours}h ${mins}m`
}

function formatRelativeTime(dateString: string): string {
  const date = new Date(dateString)
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffMins = Math.floor(diffMs / (1000 * 60))

  if (diffMins < 1) return 'Just now'
  if (diffMins < 60) return `${diffMins}m ago`
  const diffHours = Math.floor(diffMins / 60)
  if (diffHours < 24) return `${diffHours}h ago`
  const diffDays = Math.floor(diffHours / 24)
  return `${diffDays}d ago`
}

export default function Activity() {
  const queryClient = useQueryClient()

  const { data, isLoading, error } = useQuery({
    queryKey: ['activity'],
    queryFn: fetchActivity,
    refetchInterval: 2000,
  })

  const pauseMutation = useMutation({
    mutationFn: pauseDownloads,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['activity'] }),
  })

  const resumeMutation = useMutation({
    mutationFn: resumeDownloads,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['activity'] }),
  })

  const deleteMutation = useMutation({
    mutationFn: ({ id, downloaderId }: { id: string; downloaderId: string }) =>
      deleteItem(id, downloaderId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['activity'] }),
  })

  const isPaused = data?.stats?.status === 'Paused'
  const isDownloading = data?.queue && data.queue.length > 0

  return (
    <div className="min-h-screen p-6">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Activity</h1>
          <p className="text-sm text-muted">Download queue and history</p>
        </div>

        {data?.stats && (
          <div className="flex items-center gap-6">
            <div className="text-right">
              <p className="text-2xl font-bold text-accent">{formatSpeed(data.stats.speed)}</p>
              <p className="text-xs text-muted">Current Speed</p>
            </div>
            {isDownloading && (
              <div className="text-right">
                <p className="text-lg font-medium text-foreground">{data.stats.sizeLeft}</p>
                <p className="text-xs text-muted">{formatTimeLeft(data.stats.timeLeft)} left</p>
              </div>
            )}
            <button
              type="button"
              onClick={() => (isPaused ? resumeMutation.mutate() : pauseMutation.mutate())}
              disabled={pauseMutation.isPending || resumeMutation.isPending}
              className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-colors ${
                isPaused
                  ? 'bg-accent text-white hover:bg-accent/90'
                  : 'bg-yellow-500/20 text-yellow-500 hover:bg-yellow-500/30'
              }`}
            >
              {isPaused ? (
                <svg aria-hidden="true" className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M8 5v14l11-7z" />
                </svg>
              ) : (
                <svg aria-hidden="true" className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
                </svg>
              )}
              {isPaused ? 'Resume' : 'Pause'}
            </button>
          </div>
        )}
      </div>

      {isLoading && (
        <div className="flex justify-center py-12">
          <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
        </div>
      )}

      {error && (
        <div className="rounded-lg bg-red-500/10 border border-red-500/20 p-4 text-red-400">
          Failed to load activity: {(error as Error).message}
        </div>
      )}

      {data && (
        <div className="space-y-8">
          {/* Queue Section */}
          <section>
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold text-foreground">Queue</h2>
              {data.queue.length > 0 && (
                <span className="text-sm text-muted">
                  {data.queue.length} item{data.queue.length !== 1 ? 's' : ''}
                </span>
              )}
            </div>

            {data.queue.length === 0 ? (
              <div className="rounded-lg bg-card border border-border p-8 text-center">
                <div className="w-12 h-12 rounded-full bg-background flex items-center justify-center mx-auto mb-3">
                  <svg
                    aria-hidden="true"
                    className="w-6 h-6 text-muted"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={1.5}
                      d="M5 13l4 4L19 7"
                    />
                  </svg>
                </div>
                <p className="text-muted">No active downloads</p>
              </div>
            ) : (
              <div className="space-y-3">
                {data.queue.map((item, index) => (
                  <div key={item.id} className="rounded-lg bg-card border border-border p-4">
                    <div className="flex items-start justify-between gap-4 mb-3">
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2 mb-1">
                          {index === 0 && (
                            <span className="inline-flex items-center gap-1 px-1.5 py-0.5 text-[10px] font-medium uppercase bg-accent/20 text-accent rounded">
                              <span className="w-1.5 h-1.5 rounded-full bg-accent animate-pulse" />
                              Active
                            </span>
                          )}
                        </div>
                        <p className="font-medium text-foreground truncate" title={item.filename}>
                          {item.filename}
                        </p>
                        <div className="flex items-center gap-2 mt-1 text-xs text-muted">
                          <span>{item.downloader}</span>
                          <span>·</span>
                          <span>{item.category || 'Default'}</span>
                          <span>·</span>
                          <span>
                            {(item.sizeMB - item.remainingMB).toFixed(0)} / {item.sizeMB.toFixed(0)}{' '}
                            MB
                          </span>
                        </div>
                      </div>
                      <div className="flex items-center gap-3">
                        <div className="text-right">
                          <p className="text-sm font-medium text-foreground">
                            {formatTimeLeft(item.timeLeft)}
                          </p>
                          <p className="text-xs text-muted">{item.status}</p>
                        </div>
                        <button
                          type="button"
                          onClick={() =>
                            deleteMutation.mutate({ id: item.id, downloaderId: item.downloaderId })
                          }
                          className="p-1.5 rounded text-muted hover:text-red-400 hover:bg-red-500/10 transition-colors"
                          title="Remove from queue"
                        >
                          <svg
                            aria-hidden="true"
                            className="w-4 h-4"
                            fill="none"
                            stroke="currentColor"
                            viewBox="0 0 24 24"
                          >
                            <path
                              strokeLinecap="round"
                              strokeLinejoin="round"
                              strokeWidth={2}
                              d="M6 18L18 6M6 6l12 12"
                            />
                          </svg>
                        </button>
                      </div>
                    </div>

                    {/* Progress bar */}
                    <div className="flex items-center gap-3">
                      <div className="flex-1 h-2 bg-background rounded-full overflow-hidden">
                        <div
                          className="h-full bg-accent rounded-full transition-all duration-300"
                          style={{ width: `${item.percentage}%` }}
                        />
                      </div>
                      <span className="text-xs text-muted w-12 text-right">
                        {item.percentage.toFixed(1)}%
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </section>

          {/* Stats Summary */}
          {data.stats && (
            <section>
              <h2 className="text-lg font-semibold text-foreground mb-4">Statistics</h2>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div className="rounded-lg bg-card border border-border p-4">
                  <p className="text-2xl font-bold text-foreground">{data.stats.daySize}</p>
                  <p className="text-xs text-muted">Today</p>
                </div>
                <div className="rounded-lg bg-card border border-border p-4">
                  <p className="text-2xl font-bold text-foreground">{data.stats.weekSize}</p>
                  <p className="text-xs text-muted">This Week</p>
                </div>
                <div className="rounded-lg bg-card border border-border p-4">
                  <p className="text-2xl font-bold text-foreground">{data.stats.monthSize}</p>
                  <p className="text-xs text-muted">This Month</p>
                </div>
                <div className="rounded-lg bg-card border border-border p-4">
                  <p className="text-2xl font-bold text-foreground">{data.stats.totalSize}</p>
                  <p className="text-xs text-muted">All Time</p>
                </div>
              </div>
            </section>
          )}

          {/* History Section */}
          <section>
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold text-foreground">Recent History</h2>
              {data.history.length > 0 && (
                <span className="text-sm text-muted">Last {data.history.length}</span>
              )}
            </div>

            {data.history.length === 0 ? (
              <div className="rounded-lg bg-card border border-border p-8 text-center">
                <p className="text-muted">No download history</p>
              </div>
            ) : (
              <div className="rounded-lg bg-card border border-border overflow-hidden">
                <table className="w-full">
                  <thead>
                    <tr className="border-b border-border bg-background/50">
                      <th className="text-left px-4 py-3 text-xs font-medium text-muted uppercase tracking-wider">
                        Name
                      </th>
                      <th className="text-left px-4 py-3 text-xs font-medium text-muted uppercase tracking-wider">
                        Status
                      </th>
                      <th className="text-left px-4 py-3 text-xs font-medium text-muted uppercase tracking-wider">
                        Size
                      </th>
                      <th className="text-left px-4 py-3 text-xs font-medium text-muted uppercase tracking-wider">
                        Duration
                      </th>
                      <th className="text-left px-4 py-3 text-xs font-medium text-muted uppercase tracking-wider">
                        Completed
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border">
                    {data.history.map((item) => (
                      <tr key={item.id} className="hover:bg-background/30">
                        <td className="px-4 py-3">
                          <p
                            className="text-sm text-foreground truncate max-w-md"
                            title={item.name}
                          >
                            {item.name}
                          </p>
                          <p className="text-xs text-muted">{item.downloader}</p>
                        </td>
                        <td className="px-4 py-3">
                          <span
                            className={`inline-block px-2 py-0.5 text-xs font-medium rounded ${
                              item.status === 'Completed'
                                ? 'bg-green-500/20 text-green-400'
                                : 'bg-red-500/20 text-red-400'
                            }`}
                          >
                            {item.status}
                          </span>
                          {item.failMessage && (
                            <p className="text-xs text-red-400 mt-1">{item.failMessage}</p>
                          )}
                        </td>
                        <td className="px-4 py-3 text-sm text-muted">{item.size}</td>
                        <td className="px-4 py-3 text-sm text-muted">
                          {formatDownloadTime(item.downloadTime)}
                        </td>
                        <td className="px-4 py-3 text-sm text-muted">
                          {formatRelativeTime(item.completedAt)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>

          {/* Errors */}
          {data.errors && data.errors.length > 0 && (
            <div className="rounded-lg bg-yellow-500/10 border border-yellow-500/20 p-4">
              <p className="text-sm font-medium text-yellow-500 mb-2">
                Some downloaders had errors:
              </p>
              {data.errors.map((err) => (
                <p key={err.downloader} className="text-sm text-yellow-400/80">
                  {err.downloader}: {err.error}
                </p>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
