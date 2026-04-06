import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../lib/auth'
import { apiFetch, apiPost } from '../lib/api-client'

import type { RequestStatus } from '../hooks/useRequestStatus'

type RequestingUser = {
  id: string
  name: string
  requestedAt: string
}

type MediaRequest = {
  id: string
  title: string
  year: number | null
  type: 'movie' | 'tv'
  tmdbId: string | null
  imdbId: string | null
  tvdbId: string | null
  season: number | null
  posterPath: string | null
  overview: string | null
  status: RequestStatus
  minQuality: string | null
  // TV series specific
  seriesStatus: 'returning' | 'ended' | 'upcoming' | 'unknown' | null
  totalEpisodes: number | null
  downloadedEpisodes: number | null
  // Timestamps
  createdAt: string
  approvedAt: string | null
  lastSearchedAt: string | null
  completedAt: string | null
  requestedBy: RequestingUser
  requestedByUsers: RequestingUser[]
  approvedBy: { id: string; name: string } | null
}

type FilterTab = 'all' | 'pending' | 'searching' | 'completed'

async function fetchRequests(status?: string): Promise<{ requests: MediaRequest[] }> {
  const params = status ? `?status=${status}` : ''
  return apiFetch(`/api/requests${params}`)
}

async function approveRequest(id: string): Promise<void> {
  return apiPost(`/api/requests/${id}/approve`, {})
}

async function rejectRequest(id: string): Promise<void> {
  return apiPost(`/api/requests/${id}/reject`, {})
}

async function deleteRequest(id: string): Promise<void> {
  return apiFetch(`/api/requests/${id}`, { method: 'DELETE' })
}

async function runSearchJob(): Promise<{ searched: number; found: number; errors: number }> {
  return apiPost('/api/requests/search-job', {})
}

async function runMonitorJob(): Promise<{ checked: number; completed: number; failed: number }> {
  return apiPost('/api/requests/monitor-job', {})
}

async function runEpisodeSearchJob(): Promise<{ checked: number; found: number; errors: number; seasonPacks: number }> {
  return apiPost('/api/requests/episode-search-job', {})
}

async function runAirDateMonitor(): Promise<{ requestsChecked: number; episodesAdded: number; statusUpdates: number; errors: number }> {
  return apiPost('/api/requests/air-date-monitor', {})
}

async function fetchEpisodesFromTmdb(requestId: string): Promise<{ success: boolean; episodesAdded: number; seriesStatus: string }> {
  const response = await fetch(`/api/requests/${requestId}/episodes/fetch`, { method: 'POST' })
  if (!response.ok) throw new Error('Failed to fetch episodes from TMDB')
  return response.json()
}

function formatCompactTime(dateString: string | null): string {
  if (!dateString) return '-'
  const date = new Date(dateString)
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffMins = Math.floor(diffMs / (1000 * 60))

  if (diffMins < 1) return 'now'
  if (diffMins < 60) return `${diffMins}m`
  const diffHours = Math.floor(diffMins / 60)
  if (diffHours < 24) return `${diffHours}h`
  const diffDays = Math.floor(diffHours / 24)
  if (diffDays < 30) return `${diffDays}d`
  const diffMonths = Math.floor(diffDays / 30)
  if (diffMonths < 12) return `${diffMonths}mo`
  const diffYears = Math.floor(diffDays / 365)
  return `${diffYears}y`
}

function formatFullTime(dateString: string | null): string {
  if (!dateString) return 'Never'
  const date = new Date(dateString)
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffMins = Math.floor(diffMs / (1000 * 60))

  if (diffMins < 1) return 'Just now'
  if (diffMins < 60) return `${diffMins} minute${diffMins === 1 ? '' : 's'} ago`
  const diffHours = Math.floor(diffMins / 60)
  if (diffHours < 24) return `${diffHours} hour${diffHours === 1 ? '' : 's'} ago`
  const diffDays = Math.floor(diffHours / 24)
  if (diffDays < 30) return `${diffDays} day${diffDays === 1 ? '' : 's'} ago`
  const diffMonths = Math.floor(diffDays / 30)
  if (diffMonths < 12) return `${diffMonths} month${diffMonths === 1 ? '' : 's'} ago`
  const diffYears = Math.floor(diffDays / 365)
  return `${diffYears} year${diffYears === 1 ? '' : 's'} ago`
}

function SeriesStatusBadge({ status }: { status: 'returning' | 'ended' | 'upcoming' | 'unknown' }) {
  const styles: Record<typeof status, string> = {
    returning: 'bg-green-500/20 text-green-400',
    ended: 'bg-gray-500/20 text-gray-400',
    upcoming: 'bg-blue-500/20 text-blue-400',
    unknown: 'bg-gray-500/20 text-gray-400',
  }

  const labels: Record<typeof status, string> = {
    returning: 'Ongoing',
    ended: 'Ended',
    upcoming: 'Upcoming',
    unknown: 'Unknown',
  }

  return (
    <span className={`inline-block px-1.5 py-0.5 text-[10px] font-medium rounded ${styles[status]}`}>
      {labels[status]}
    </span>
  )
}

function EpisodeProgress({ downloaded, total }: { downloaded: number; total: number }) {
  const percentage = total > 0 ? Math.round((downloaded / total) * 100) : 0

  return (
    <div className="flex items-center gap-2">
      <div className="flex-1 h-1.5 bg-background rounded-full overflow-hidden">
        <div
          className="h-full bg-accent transition-all duration-300"
          style={{ width: `${percentage}%` }}
        />
      </div>
      <span className="text-xs text-muted whitespace-nowrap">
        {downloaded}/{total}
      </span>
    </div>
  )
}

function StatusBadge({ status }: { status: RequestStatus }) {
  const styles: Record<RequestStatus, string> = {
    pending_approval: 'bg-yellow-500/20 text-yellow-400',
    approved: 'bg-blue-500/20 text-blue-400',
    searching: 'bg-purple-500/20 text-purple-400',
    found: 'bg-cyan-500/20 text-cyan-400',
    fetching_nzb: 'bg-cyan-500/20 text-cyan-400',
    nzb_downloaded: 'bg-cyan-500/20 text-cyan-400',
    sent_to_downloader: 'bg-accent/20 text-accent',
    downloading: 'bg-accent/20 text-accent',
    download_complete: 'bg-emerald-500/20 text-emerald-400',
    importing: 'bg-emerald-500/20 text-emerald-400',
    completed: 'bg-green-500/20 text-green-400',
    rejected: 'bg-red-500/20 text-red-400',
    failed: 'bg-red-500/20 text-red-400',
  }

  const labels: Record<RequestStatus, string> = {
    pending_approval: 'Pending',
    approved: 'Approved',
    searching: 'Searching',
    found: 'Found',
    fetching_nzb: 'Fetching NZB',
    nzb_downloaded: 'NZB Ready',
    sent_to_downloader: 'Sent',
    downloading: 'Downloading',
    download_complete: 'Downloaded',
    importing: 'Importing',
    completed: 'Completed',
    rejected: 'Rejected',
    failed: 'Failed',
  }

  return (
    <span className={`inline-block px-2 py-0.5 text-xs font-medium rounded ${styles[status]}`}>
      {labels[status]}
    </span>
  )
}

export default function RequestsList() {
  const { isAdmin } = useAuth()
  const queryClient = useQueryClient()
  const [activeTab, setActiveTab] = useState<FilterTab>('all')

  const statusFilter = activeTab === 'all' ? undefined : activeTab === 'pending' ? 'pending_approval' : activeTab === 'searching' ? 'approved,searching,found,fetching_nzb,nzb_downloaded,sent_to_downloader,downloading,download_complete,importing' : 'completed'

  const { data, isLoading, error } = useQuery({
    queryKey: ['requests', statusFilter],
    queryFn: () => fetchRequests(statusFilter),
    refetchInterval: 10000,
  })

  const approveMutation = useMutation({
    mutationFn: approveRequest,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['requests'] }),
  })

  const rejectMutation = useMutation({
    mutationFn: rejectRequest,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['requests'] }),
  })

  const deleteMutation = useMutation({
    mutationFn: deleteRequest,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['requests'] }),
  })

  const searchJobMutation = useMutation({
    mutationFn: runSearchJob,
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['requests'] })
      alert(`Search complete: ${result.found} found, ${result.searched} searched, ${result.errors} errors`)
    },
  })

  const monitorJobMutation = useMutation({
    mutationFn: runMonitorJob,
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['requests'] })
      alert(`Monitor complete: ${result.completed} completed, ${result.failed} failed, ${result.checked} checked`)
    },
  })

  const episodeSearchMutation = useMutation({
    mutationFn: runEpisodeSearchJob,
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['requests'] })
      alert(`Episode search complete: ${result.found} found, ${result.seasonPacks} season packs, ${result.errors} errors`)
    },
  })

  const airDateMonitorMutation = useMutation({
    mutationFn: runAirDateMonitor,
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['requests'] })
      alert(`Air date monitor complete: ${result.episodesAdded} episodes added, ${result.statusUpdates} status updates`)
    },
  })

  const fetchEpisodesMutation = useMutation({
    mutationFn: fetchEpisodesFromTmdb,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['requests'] })
      queryClient.invalidateQueries({ queryKey: ['episodes'] })
    },
  })

  const tabs: { key: FilterTab; label: string }[] = [
    { key: 'all', label: 'All' },
    { key: 'pending', label: 'Pending' },
    { key: 'searching', label: 'Searching' },
    { key: 'completed', label: 'Completed' },
  ]

  return (
    <div className="min-h-screen p-6">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Requests</h1>
          <p className="text-sm text-muted">Manage media requests</p>
        </div>

        {isAdmin && (
          <div className="flex items-center gap-2 flex-wrap justify-end">
            <button
              type="button"
              onClick={() => airDateMonitorMutation.mutate()}
              disabled={airDateMonitorMutation.isPending}
              className="flex items-center gap-2 rounded-md bg-card border border-border px-3 py-2 text-sm font-medium text-foreground hover:bg-background disabled:opacity-50 transition-colors"
              title="Check for new episodes from TMDB"
            >
              {airDateMonitorMutation.isPending ? (
                <div className="h-4 w-4 animate-spin rounded-full border-2 border-foreground border-t-transparent" />
              ) : (
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                </svg>
              )}
              <span className="hidden sm:inline">Air Dates</span>
            </button>
            <button
              type="button"
              onClick={() => monitorJobMutation.mutate()}
              disabled={monitorJobMutation.isPending}
              className="flex items-center gap-2 rounded-md bg-card border border-border px-3 py-2 text-sm font-medium text-foreground hover:bg-background disabled:opacity-50 transition-colors"
              title="Check download status"
            >
              {monitorJobMutation.isPending ? (
                <div className="h-4 w-4 animate-spin rounded-full border-2 border-foreground border-t-transparent" />
              ) : (
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                </svg>
              )}
              <span className="hidden sm:inline">Downloads</span>
            </button>
            <button
              type="button"
              onClick={() => episodeSearchMutation.mutate()}
              disabled={episodeSearchMutation.isPending}
              className="flex items-center gap-2 rounded-md bg-card border border-border px-3 py-2 text-sm font-medium text-foreground hover:bg-background disabled:opacity-50 transition-colors"
              title="Search for TV episodes"
            >
              {episodeSearchMutation.isPending ? (
                <div className="h-4 w-4 animate-spin rounded-full border-2 border-foreground border-t-transparent" />
              ) : (
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z" />
                </svg>
              )}
              <span className="hidden sm:inline">Episodes</span>
            </button>
            <button
              type="button"
              onClick={() => searchJobMutation.mutate()}
              disabled={searchJobMutation.isPending}
              className="flex items-center gap-2 rounded-md bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent/90 disabled:opacity-50 transition-colors"
              title="Search for all approved requests"
            >
              {searchJobMutation.isPending ? (
                <div className="h-4 w-4 animate-spin rounded-full border-2 border-white border-t-transparent" />
              ) : (
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                </svg>
              )}
              <span className="hidden sm:inline">Search</span>
            </button>
          </div>
        )}
      </div>

      {/* Tabs */}
      <div className="flex gap-1 mb-6 border-b border-border">
        {tabs.map((tab) => (
          <button
            key={tab.key}
            type="button"
            onClick={() => setActiveTab(tab.key)}
            className={`px-4 py-2 text-sm font-medium transition-colors border-b-2 -mb-px ${
              activeTab === tab.key
                ? 'text-accent border-accent'
                : 'text-muted border-transparent hover:text-foreground'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {isLoading && (
        <div className="flex justify-center py-12">
          <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
        </div>
      )}

      {error && (
        <div className="rounded-lg bg-red-500/10 border border-red-500/20 p-4 text-red-400">
          Failed to load requests: {(error as Error).message}
        </div>
      )}

      {data && data.requests.length === 0 && (
        <div className="rounded-lg bg-card border border-border p-8 text-center">
          <div className="w-12 h-12 rounded-full bg-background flex items-center justify-center mx-auto mb-3">
            <svg className="w-6 h-6 text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
            </svg>
          </div>
          <p className="text-muted">No requests found</p>
        </div>
      )}

      {data && data.requests.length > 0 && (
        <div className="space-y-3">
          {data.requests.map((request) => (
            <div key={request.id} className="rounded-lg bg-card border border-border p-4">
              <div className="flex gap-4">
                {/* Poster */}
                <div className="w-16 h-24 rounded bg-background overflow-hidden flex-shrink-0">
                  {request.posterPath ? (
                    <img
                      src={`https://image.tmdb.org/t/p/w92${request.posterPath}`}
                      alt={request.title}
                      className="w-full h-full object-cover"
                    />
                  ) : (
                    <div className="w-full h-full flex items-center justify-center">
                      <svg className="w-6 h-6 text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z" />
                      </svg>
                    </div>
                  )}
                </div>

                {/* Info */}
                <div className="flex-1 min-w-0">
                  <div className="flex items-start justify-between gap-4">
                    <div>
                      <h3 className="font-medium text-foreground truncate" title={`${request.title}${request.year ? ` (${request.year})` : ''}`}>
                        {request.title}
                        {request.year && <span className="text-muted ml-1">({request.year})</span>}
                      </h3>
                      <div className="flex items-center gap-1.5 mt-1 text-xs text-muted flex-wrap">
                        <span className="capitalize">{request.type}</span>
                        {request.season && <span>· Season {request.season}</span>}
                        {request.seriesStatus && (
                          <>
                            <span>·</span>
                            <SeriesStatusBadge status={request.seriesStatus} />
                          </>
                        )}
                        <span>
                          · by{' '}
                          {request.requestedByUsers.length > 0
                            ? request.requestedByUsers.length === 1
                              ? request.requestedByUsers[0]?.name
                              : `${request.requestedByUsers[0]?.name} +${request.requestedByUsers.length - 1} more`
                            : request.requestedBy.name}
                        </span>
                      </div>
                      {request.requestedByUsers.length > 1 && (
                        <div className="flex flex-wrap gap-1 mt-1.5">
                          {request.requestedByUsers.map((user) => (
                            <span
                              key={user.id}
                              className="px-1.5 py-0.5 text-[10px] rounded-full bg-accent/10 text-accent"
                            >
                              {user.name}
                            </span>
                          ))}
                        </div>
                      )}
                      {/* Episode progress for TV requests */}
                      {request.type === 'tv' && request.season && request.totalEpisodes !== null && request.totalEpisodes > 0 && (
                        <div className="mt-2 max-w-xs">
                          <EpisodeProgress
                            downloaded={request.downloadedEpisodes ?? 0}
                            total={request.totalEpisodes}
                          />
                        </div>
                      )}
                    </div>
                    <StatusBadge status={request.status} />
                  </div>

                  <div className="flex items-center gap-4 mt-3 text-xs text-muted">
                    <span title={formatFullTime(request.createdAt)}>
                      Requested {formatCompactTime(request.createdAt)}
                    </span>
                    {request.minQuality && (
                      <span className="px-1.5 py-0.5 rounded bg-black/30 text-foreground">
                        Min: {request.minQuality}
                      </span>
                    )}
                    {request.lastSearchedAt && (
                      <span title={formatFullTime(request.lastSearchedAt)}>
                        · Last searched {formatCompactTime(request.lastSearchedAt)}
                      </span>
                    )}
                  </div>

                  {/* Actions */}
                  <div className="flex items-center gap-2 mt-3 flex-wrap">
                    {isAdmin && request.status === 'pending_approval' && (
                      <>
                        <button
                          type="button"
                          onClick={() => approveMutation.mutate(request.id)}
                          disabled={approveMutation.isPending}
                          className="px-3 py-1 text-xs font-medium rounded bg-green-500/20 text-green-400 hover:bg-green-500/30 transition-colors"
                        >
                          Approve
                        </button>
                        <button
                          type="button"
                          onClick={() => rejectMutation.mutate(request.id)}
                          disabled={rejectMutation.isPending}
                          className="px-3 py-1 text-xs font-medium rounded bg-red-500/20 text-red-400 hover:bg-red-500/30 transition-colors"
                        >
                          Reject
                        </button>
                      </>
                    )}
                    {/* Fetch episodes button for TV requests without episodes */}
                    {isAdmin && request.type === 'tv' && request.season && request.tmdbId && (request.totalEpisodes === null || request.totalEpisodes === 0) && (
                      <button
                        type="button"
                        onClick={() => fetchEpisodesMutation.mutate(request.id)}
                        disabled={fetchEpisodesMutation.isPending}
                        className="px-3 py-1 text-xs font-medium rounded bg-accent/20 text-accent hover:bg-accent/30 transition-colors"
                      >
                        {fetchEpisodesMutation.isPending ? 'Fetching...' : 'Fetch Episodes'}
                      </button>
                    )}
                    {isAdmin ? (
                      <button
                        type="button"
                        onClick={() => {
                          if (confirm('Delete this request for all users?')) {
                            deleteMutation.mutate(request.id)
                          }
                        }}
                        disabled={deleteMutation.isPending}
                        className="px-3 py-1 text-xs font-medium rounded text-muted hover:text-red-400 hover:bg-red-500/10 transition-colors"
                      >
                        Delete
                      </button>
                    ) : (
                      <button
                        type="button"
                        onClick={() => {
                          if (confirm('Remove yourself from this request?')) {
                            deleteMutation.mutate(request.id)
                          }
                        }}
                        disabled={deleteMutation.isPending}
                        className="px-3 py-1 text-xs font-medium rounded text-muted hover:text-foreground hover:bg-background transition-colors"
                      >
                        Unrequest
                      </button>
                    )}
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
