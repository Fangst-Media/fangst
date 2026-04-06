import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useState } from 'react'
import {
  MonitoringPresetSelector,
  type MonitoringPreset,
} from '../components/MonitoringPresetSelector'
import { tvLanes, apiFetch, apiPost } from '../lib/api-client'

type EpisodeFile = {
  id: string
  filename: string
  resolution?: string
  size?: number
  videoCodec?: string
  audioCodec?: string
  hdr?: string
  source?: string
}

type EpisodeWithFile = {
  episodeNumber: number
  title?: string
  overview?: string
  airDate?: string
  downloaded: boolean
  file?: EpisodeFile
}

type SeasonWithEpisodes = {
  seasonNumber: number
  name: string
  airDate?: string
  episodes: EpisodeWithFile[]
}

type SeriesBrief = {
  tmdbId: string
  title: string
  posterPath?: string
  status?: 'returning' | 'ended' | 'upcoming' | 'unknown'
}

type RequestedEpisode = {
  id: string
  seasonNumber: number
  episodeNumber: number
  title?: string
  status: string
  monitored: boolean
}

type ExistingRequest = {
  id: string
  status: string
  monitoringPreset?: string
}

type SeasonRequestResponse = {
  request: ExistingRequest | null
  episodes: RequestedEpisode[]
}

async function getSeasonEpisodes(tmdbId: string, seasonNumber: number): Promise<SeasonWithEpisodes> {
  return apiFetch(`/api/library/tv/${tmdbId}/season/${seasonNumber}`)
}

async function getSeriesBrief(tmdbId: string): Promise<SeriesBrief> {
  const data = await tvLanes.getById(tmdbId)
  return {
    tmdbId: data.id,
    title: data.title,
    posterPath: data.poster,
    status: data.status,
  }
}

async function getSeasonRequest(
  tmdbId: string,
  seasonNumber: number
): Promise<SeasonRequestResponse> {
  return apiFetch(`/api/requests/season/${tmdbId}/${seasonNumber}`)
}

async function createRequest(data: {
  title: string
  year?: number
  type: 'movie' | 'tv'
  tmdbId: string
  posterPath?: string
  overview?: string
  season: number
  seriesStatus?: string
  monitoringPreset?: MonitoringPreset
  fetchEpisodes?: boolean
}): Promise<{ success: boolean; request: { id: string } }> {
  return apiPost('/api/requests', data)
}

async function fetchEpisodesForRequest(requestId: string): Promise<void> {
  return apiPost(`/api/requests/${requestId}/episodes/fetch`, {})
}

async function toggleEpisodeMonitoring(
  requestId: string,
  episodeId: string,
  monitored: boolean
): Promise<void> {
  return apiFetch(`/api/requests/${requestId}/episodes/${episodeId}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ monitored }),
  })
}

function formatBytes(bytes?: number): string {
  if (!bytes || bytes === 0) return '-'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

function formatAirDate(dateString?: string): string {
  if (!dateString) return '-'
  const date = new Date(dateString)
  return date.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })
}

function isAired(dateString?: string): boolean {
  if (!dateString) return true
  return new Date(dateString) <= new Date()
}

export default function SeasonPage({
  tmdbId,
  seasonNumber,
}: {
  tmdbId: string
  seasonNumber: number
}) {
  const queryClient = useQueryClient()
  const [showRequestModal, setShowRequestModal] = useState(false)
  const [monitoringPreset, setMonitoringPreset] = useState<MonitoringPreset>('all')
  const [requestingEpisode, setRequestingEpisode] = useState<number | null>(null)

  const { data: season, isLoading, error } = useQuery({
    queryKey: ['library-season', tmdbId, seasonNumber],
    queryFn: () => getSeasonEpisodes(tmdbId, seasonNumber),
  })

  const { data: series } = useQuery({
    queryKey: ['series-brief', tmdbId],
    queryFn: () => getSeriesBrief(tmdbId),
  })

  const { data: existingRequestData, refetch: refetchRequest } = useQuery({
    queryKey: ['season-request', tmdbId, seasonNumber],
    queryFn: () => getSeasonRequest(tmdbId, seasonNumber),
  })

  const existingRequest = existingRequestData?.request
  const requestedEpisodes = existingRequestData?.episodes || []

  const requestMutation = useMutation({
    mutationFn: async () => {
      const result = await createRequest({
        title: series?.title || 'Unknown Series',
        type: 'tv',
        tmdbId,
        posterPath: series?.posterPath,
        season: seasonNumber,
        seriesStatus: series?.status,
        monitoringPreset,
        fetchEpisodes: true,
      })
      return result
    },
    onSuccess: async (data) => {
      await fetchEpisodesForRequest(data.request.id)
      queryClient.invalidateQueries({ queryKey: ['season-request', tmdbId, seasonNumber] })
      setShowRequestModal(false)
    },
  })

  const toggleMonitoringMutation = useMutation({
    mutationFn: async ({
      episodeId,
      monitored,
    }: {
      episodeId: string
      monitored: boolean
    }) => {
      if (!existingRequest) return
      await toggleEpisodeMonitoring(existingRequest.id, episodeId, monitored)
    },
    onSuccess: () => {
      refetchRequest()
    },
  })

  const requestSingleEpisodeMutation = useMutation({
    mutationFn: async (episodeNumber: number) => {
      setRequestingEpisode(episodeNumber)

      if (existingRequest) {
        // Check if episodes exist, if not fetch them first
        let episodes = requestedEpisodes
        if (episodes.length === 0) {
          await fetchEpisodesForRequest(existingRequest.id)
          // Re-fetch the request data to get the new episodes
          const updated = await getSeasonRequest(tmdbId, seasonNumber)
          episodes = updated.episodes
        }

        // Find the episode and toggle monitoring
        const episode = episodes.find((e) => e.episodeNumber === episodeNumber)
        if (episode) {
          await toggleEpisodeMonitoring(existingRequest.id, episode.id, true)
        }
      } else {
        // Create new request
        const result = await createRequest({
          title: series?.title || 'Unknown Series',
          type: 'tv',
          tmdbId,
          posterPath: series?.posterPath,
          season: seasonNumber,
          seriesStatus: series?.status,
          monitoringPreset: 'all',
          fetchEpisodes: true,
        })
        await fetchEpisodesForRequest(result.request.id)
      }
    },
    onSuccess: () => {
      setRequestingEpisode(null)
      refetchRequest()
    },
    onError: () => {
      setRequestingEpisode(null)
    },
  })

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="text-center">
          <div className="mb-4 h-10 w-10 animate-spin rounded-full border-2 border-accent border-t-transparent mx-auto" />
          <p className="text-sm text-muted">Loading season...</p>
        </div>
      </div>
    )
  }

  if (error || !season) {
    return (
      <div className="flex min-h-screen items-center justify-center p-4">
        <div className="rounded-lg bg-card border border-border p-6 max-w-md text-center">
          <div className="w-12 h-12 rounded-full bg-red-500/10 flex items-center justify-center mx-auto mb-4">
            <svg className="w-6 h-6 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
              />
            </svg>
          </div>
          <h2 className="text-lg font-semibold text-foreground mb-2">Season not found</h2>
          <p className="text-sm text-muted mb-4">
            {error instanceof Error ? error.message : 'Could not load this season'}
          </p>
          <Link to="/library" className="text-accent hover:text-accent/80 text-sm font-medium">
            Back to Library
          </Link>
        </div>
      </div>
    )
  }

  const downloadedCount = season.episodes.filter((e) => e.downloaded).length
  const totalCount = season.episodes.length
  const missingCount = season.episodes.filter((e) => !e.downloaded && isAired(e.airDate)).length
  const progressPercent = totalCount > 0 ? (downloadedCount / totalCount) * 100 : 0

  return (
    <div className="min-h-screen p-6">
      {/* Request Modal */}
      {showRequestModal && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
          onClick={() => setShowRequestModal(false)}
          onKeyDown={(e) => e.key === 'Escape' && setShowRequestModal(false)}
          role="dialog"
          aria-modal="true"
        >
          <div
            className="relative w-full max-w-sm m-4 rounded-xl bg-card border border-border shadow-2xl overflow-hidden"
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => e.stopPropagation()}
          >
            <div className="p-4 border-b border-border">
              <h3 className="text-lg font-semibold text-foreground">Request Missing Episodes</h3>
              <p className="text-sm text-muted mt-1">
                {series?.title} - {season.name}
              </p>
            </div>

            <div className="p-4 space-y-4">
              <p className="text-sm text-muted">
                <span className="text-foreground font-medium">{missingCount}</span> missing episodes
                will be requested
              </p>

              <MonitoringPresetSelector
                value={monitoringPreset}
                onChange={setMonitoringPreset}
                seriesStatus={series?.status}
              />
            </div>

            <div className="flex gap-3 p-4 border-t border-border">
              <button
                type="button"
                onClick={() => setShowRequestModal(false)}
                className="flex-1 rounded-md px-4 py-2.5 text-sm font-medium text-muted border border-border hover:text-foreground transition-colors"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => requestMutation.mutate()}
                disabled={requestMutation.isPending}
                className="flex-1 rounded-md bg-accent px-4 py-2.5 text-sm font-medium text-white hover:bg-accent/90 transition-colors disabled:opacity-50"
              >
                {requestMutation.isPending ? 'Requesting...' : 'Request'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Breadcrumb */}
      <nav className="flex items-center gap-2 text-sm mb-6">
        <Link to="/library" className="text-muted hover:text-foreground transition-colors">
          Library
        </Link>
        <span className="text-muted">/</span>
        <Link
          to="/library/tv/$tmdbId"
          params={{ tmdbId }}
          className="text-muted hover:text-foreground transition-colors"
        >
          {series?.title || 'Series'}
        </Link>
        <span className="text-muted">/</span>
        <span className="text-foreground">{season.name}</span>
      </nav>

      {/* Header */}
      <div className="mb-8">
        <div className="flex items-start justify-between gap-4">
          <div className="flex-1">
            <h1 className="text-2xl font-bold text-foreground mb-2">{season.name}</h1>
            <div className="flex items-center gap-4 text-sm">
              <span className="text-muted">{totalCount} episodes</span>
              {season.airDate && (
                <span className="text-muted">Premiered {formatAirDate(season.airDate)}</span>
              )}
              {existingRequest && (
                <span className="px-2 py-0.5 rounded-full bg-accent/10 text-accent text-xs font-medium">
                  Requested
                </span>
              )}
            </div>
          </div>

          {/* Request button */}
          {missingCount > 0 && !existingRequest && (
            <button
              type="button"
              onClick={() => setShowRequestModal(true)}
              className="shrink-0 flex items-center gap-2 rounded-md bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent/90 transition-colors"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 4v16m8-8H4"
                />
              </svg>
              Request Missing ({missingCount})
            </button>
          )}
        </div>

        {/* Progress */}
        <div className="mt-4 max-w-md">
          <div className="flex items-center justify-between text-sm mb-2">
            <span className="text-muted">Downloaded</span>
            <span className="text-foreground font-medium">
              {downloadedCount} / {totalCount}
            </span>
          </div>
          <div className="h-2 bg-card rounded-full overflow-hidden">
            <div
              className={`h-full transition-all duration-300 ${
                progressPercent === 100 ? 'bg-green-500' : 'bg-accent'
              }`}
              style={{ width: `${progressPercent}%` }}
            />
          </div>
        </div>
      </div>

      {/* Episodes table */}
      <div className="rounded-lg border border-border overflow-hidden">
        <table className="w-full">
          <thead className="bg-card">
            <tr className="text-left text-xs font-medium text-muted uppercase tracking-wider">
              <th className="px-4 py-3 w-16">#</th>
              <th className="px-4 py-3">Title</th>
              <th className="px-4 py-3 w-32">Air Date</th>
              <th className="px-4 py-3 w-24">Status</th>
              <th className="px-4 py-3 w-48">Quality</th>
              <th className="px-4 py-3 w-24 text-right">Size</th>
              <th className="px-4 py-3 w-24 text-right">Action</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {season.episodes.map((episode) => {
              const aired = isAired(episode.airDate)

              return (
                <tr
                  key={episode.episodeNumber}
                  className={`hover:bg-card/50 transition-colors ${!aired ? 'opacity-60' : ''}`}
                >
                  <td className="px-4 py-3">
                    <span className="text-sm font-mono text-muted">
                      {String(episode.episodeNumber).padStart(2, '0')}
                    </span>
                  </td>
                  <td className="px-4 py-3">
                    <div>
                      <p className="text-sm font-medium text-foreground">
                        {episode.title || `Episode ${episode.episodeNumber}`}
                      </p>
                      {episode.overview && (
                        <p className="text-xs text-muted line-clamp-1 mt-0.5">{episode.overview}</p>
                      )}
                    </div>
                  </td>
                  <td className="px-4 py-3">
                    <span className="text-sm text-muted">{formatAirDate(episode.airDate)}</span>
                  </td>
                  <td className="px-4 py-3">
                    {episode.downloaded ? (
                      <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-green-500/10 text-green-400 text-xs font-medium">
                        <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                          <path
                            fillRule="evenodd"
                            d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z"
                            clipRule="evenodd"
                          />
                        </svg>
                        Downloaded
                      </span>
                    ) : !aired ? (
                      <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-blue-500/10 text-blue-400 text-xs font-medium">
                        <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            strokeWidth={2}
                            d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
                          />
                        </svg>
                        Upcoming
                      </span>
                    ) : (
                      <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-red-500/10 text-red-400 text-xs font-medium">
                        <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                          <path
                            fillRule="evenodd"
                            d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z"
                            clipRule="evenodd"
                          />
                        </svg>
                        Missing
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-3">
                    {episode.file ? (
                      <div className="flex items-center gap-1.5 flex-wrap">
                        {episode.file.resolution && (
                          <span className="px-1.5 py-0.5 rounded bg-accent/10 text-accent text-xs font-medium">
                            {episode.file.resolution}
                          </span>
                        )}
                        {episode.file.hdr && (
                          <span className="px-1.5 py-0.5 rounded bg-yellow-500/10 text-yellow-400 text-xs font-medium">
                            {episode.file.hdr}
                          </span>
                        )}
                        {episode.file.videoCodec && (
                          <span className="px-1.5 py-0.5 rounded bg-card text-muted text-xs">
                            {episode.file.videoCodec}
                          </span>
                        )}
                        {episode.file.source && (
                          <span className="px-1.5 py-0.5 rounded bg-card text-muted text-xs">
                            {episode.file.source}
                          </span>
                        )}
                      </div>
                    ) : (
                      <span className="text-xs text-muted">-</span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-right">
                    <span className="text-sm text-muted">{formatBytes(episode.file?.size)}</span>
                  </td>
                  <td className="px-4 py-3 text-right">
                    {!episode.downloaded && aired && (() => {
                      const requestedEp = requestedEpisodes.find(
                        (e) => e.episodeNumber === episode.episodeNumber
                      )
                      const isMonitored = requestedEp?.monitored

                      if (isMonitored) {
                        return (
                          <span className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded bg-green-500/10 text-green-400">
                            <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                              <path
                                fillRule="evenodd"
                                d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z"
                                clipRule="evenodd"
                              />
                            </svg>
                            Monitored
                          </span>
                        )
                      }

                      return (
                        <button
                          type="button"
                          onClick={() => requestSingleEpisodeMutation.mutate(episode.episodeNumber)}
                          disabled={requestingEpisode === episode.episodeNumber}
                          className="text-xs px-2 py-1 rounded bg-accent text-white hover:bg-accent/90 transition-colors disabled:opacity-50"
                        >
                          {requestingEpisode === episode.episodeNumber ? '...' : 'Request'}
                        </button>
                      )
                    })()}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </div>
  )
}
