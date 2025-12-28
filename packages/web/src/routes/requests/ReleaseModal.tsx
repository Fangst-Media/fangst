import type { ParsedReleaseInfo } from '@jaeger/shared/types/indexer'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { QualityDetails, QualitySummary } from '../../components/quality'

export type SearchResult = {
  id: string
  title: string
  year?: number
  releaseDate?: string
  overview?: string
  posterPath?: string
  type: 'movie' | 'tv'
}

type IndexerResult = {
  guid: string
  title: string
  indexer: string
  indexerId: string
  downloadUrl: string
  size: number
  pubDate: string
  category?: string
  grabs?: number
  parsed?: ParsedReleaseInfo
}

type Downloader = {
  id: string
  name: string
  type: string
  enabled: boolean
}

type TvSeasonInfo = {
  seasonNumber: number
  name: string
  episodeCount: number
  airDate?: string
}

type TvDetailsResponse = {
  id: string
  title: string
  status: 'returning' | 'ended' | 'upcoming' | 'unknown'
  numberOfSeasons: number
  seasons: TvSeasonInfo[]
}

type QualityFilter = '2160p' | '1080p' | '720p' | 'all'

async function getMovieDetails(tmdbId: string): Promise<{ imdbId?: string; runtime?: number }> {
  const response = await fetch(`/api/metadata/tmdb/movies/${tmdbId}`)
  if (!response.ok) throw new Error('Failed to fetch movie details')
  const data = await response.json()
  return { imdbId: data.imdb_id, runtime: data.runtime ?? undefined }
}

async function getTvDetails(tmdbId: string): Promise<TvDetailsResponse> {
  const response = await fetch(`/api/metadata/tmdb/tv/${tmdbId}`)
  if (!response.ok) throw new Error('Failed to fetch TV details')
  return response.json()
}

async function searchIndexers(params: {
  imdbId?: string
  query?: string
  type: 'movie' | 'tv'
  runtime?: number
  season?: number
}): Promise<{ results: IndexerResult[]; errors?: { indexer: string; error: string }[] }> {
  const response = await fetch('/api/search', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(params),
  })
  if (!response.ok) {
    const error = await response.json()
    throw new Error(error.error || 'Search failed')
  }
  return response.json()
}

async function getDownloaders(): Promise<Downloader[]> {
  const response = await fetch('/api/downloaders')
  if (!response.ok) throw new Error('Failed to fetch downloaders')
  const data = await response.json()
  return data.downloaders.filter((d: Downloader) => d.enabled)
}

async function sendToDownloader(
  downloadUrl: string,
  downloaderId: string,
  releaseName: string,
  category?: string
): Promise<{ success: boolean; nzoId?: string }> {
  const response = await fetch('/api/search/download', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ downloadUrl, downloaderId, releaseName, category }),
  })
  if (!response.ok) {
    const error = await response.json()
    throw new Error(error.error || 'Download failed')
  }
  return response.json()
}

async function createRequest(data: {
  title: string
  year?: number
  type: 'movie' | 'tv'
  tmdbId?: string
  imdbId?: string
  posterPath?: string
  overview?: string
  season?: number
  downloading?: boolean
}): Promise<{ success: boolean }> {
  const response = await fetch('/api/requests', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!response.ok) {
    const error = await response.json()
    throw new Error(error.error || 'Failed to create request')
  }
  return response.json()
}

async function requestSeasonWithEpisodes(data: {
  title: string
  year?: number
  tmdbId: string
  posterPath?: string
  overview?: string
  season: number
  seriesStatus: 'returning' | 'ended' | 'upcoming' | 'unknown'
}): Promise<{ success: boolean; request: { id: string } }> {
  const response = await fetch('/api/requests', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ...data, type: 'tv', fetchEpisodes: true }),
  })
  if (!response.ok) {
    const error = await response.json()
    throw new Error(error.error || 'Failed to create request')
  }
  return response.json()
}

function formatBytes(bytes: number): string {
  if (!bytes || bytes === 0) return '-'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

function matchesQualityFilter(parsed: ParsedReleaseInfo | undefined, filter: QualityFilter): boolean {
  if (filter === 'all') return true
  if (!parsed?.resolution) return false
  return parsed.resolution === filter
}

function formatAge(dateString: string): string {
  const date = new Date(dateString)
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24))

  if (diffDays === 0) return 'Today'
  if (diffDays === 1) return '1 day ago'
  if (diffDays < 30) return `${diffDays} days ago`
  if (diffDays < 365) {
    const months = Math.floor(diffDays / 30)
    return `${months} ${months === 1 ? 'month' : 'months'} ago`
  }
  const years = Math.floor(diffDays / 365)
  return `${years} ${years === 1 ? 'year' : 'years'} ago`
}

export function ReleaseModal({
  media,
  onClose,
  canSwitchMode,
  onSwitchMode,
}: {
  media: SearchResult
  onClose: () => void
  canSwitchMode?: boolean
  onSwitchMode?: () => void
}) {
  const queryClient = useQueryClient()
  const [selectedDownloader, setSelectedDownloader] = useState<string>('')
  const [downloadingGuid, setDownloadingGuid] = useState<string | null>(null)
  const [successMessage, setSuccessMessage] = useState<string | null>(null)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [qualityFilter, setQualityFilter] = useState<QualityFilter>('all')
  const [selectedSeason, setSelectedSeason] = useState<number | undefined>(undefined)
  const [imdbId, setImdbId] = useState<string | undefined>(undefined)
  const [expandedGuid, setExpandedGuid] = useState<string | null>(null)

  const { data: downloaders } = useQuery({
    queryKey: ['downloaders'],
    queryFn: getDownloaders,
  })

  const { data: tvDetails } = useQuery({
    queryKey: ['tv-details', media.id],
    queryFn: () => getTvDetails(media.id),
    enabled: media.type === 'tv',
  })

  const {
    data: indexerResults,
    isLoading: isSearching,
    error: searchError,
  } = useQuery({
    queryKey: ['indexer-search', media.id, media.type, selectedSeason],
    queryFn: async () => {
      if (media.type === 'movie') {
        const details = await getMovieDetails(media.id)
        if (details.imdbId) {
          setImdbId(details.imdbId)
          return searchIndexers({ imdbId: details.imdbId, type: 'movie', runtime: details.runtime })
        }
        const query = media.year ? `${media.title} ${media.year}` : media.title
        return searchIndexers({ query, type: 'movie', runtime: details.runtime })
      }
      const query = media.year ? `${media.title} ${media.year}` : media.title
      return searchIndexers({ query, type: 'tv', season: selectedSeason })
    },
  })

  const requestMutation = useMutation({
    mutationFn: createRequest,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['requests'] })
      setSuccessMessage("Added to requests! We'll keep searching.")
      setTimeout(() => {
        setSuccessMessage(null)
        onClose()
      }, 2000)
    },
    onError: (error: Error) => {
      setErrorMessage(error.message)
      setTimeout(() => setErrorMessage(null), 5000)
    },
  })

  function handleAddToRequests() {
    requestMutation.mutate({
      title: media.title,
      year: media.year,
      type: media.type,
      tmdbId: media.id,
      imdbId,
      posterPath: media.posterPath,
      overview: media.overview,
      season: media.type === 'tv' ? selectedSeason : undefined,
    })
  }

  const seasonRequestMutation = useMutation({
    mutationFn: requestSeasonWithEpisodes,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['requests'] })
      setSuccessMessage('Season requested! Episodes will be searched automatically.')
      setTimeout(() => {
        setSuccessMessage(null)
        onClose()
      }, 2000)
    },
    onError: (error: Error) => {
      setErrorMessage(error.message)
      setTimeout(() => setErrorMessage(null), 5000)
    },
  })

  function handleRequestSeason() {
    if (!selectedSeason || !tvDetails) return
    seasonRequestMutation.mutate({
      title: media.title,
      year: media.year,
      tmdbId: media.id,
      posterPath: media.posterPath,
      overview: media.overview,
      season: selectedSeason,
      seriesStatus: tvDetails.status,
    })
  }

  const downloadMutation = useMutation({
    mutationFn: async ({
      downloadUrl,
      downloaderId,
      releaseName,
      category,
    }: { downloadUrl: string; downloaderId: string; releaseName: string; category?: string }) => {
      await createRequest({
        title: media.title,
        year: media.year,
        type: media.type,
        tmdbId: media.id,
        imdbId,
        posterPath: media.posterPath,
        overview: media.overview,
        season: media.type === 'tv' ? selectedSeason : undefined,
        downloading: true,
      })
      return sendToDownloader(downloadUrl, downloaderId, releaseName, category)
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['requests'] })
      setSuccessMessage('Download sent and added to requests!')
      setDownloadingGuid(null)
      setTimeout(() => setSuccessMessage(null), 3000)
    },
    onError: (error: Error) => {
      setErrorMessage(error.message)
      setDownloadingGuid(null)
      setTimeout(() => setErrorMessage(null), 5000)
    },
  })

  function handleDownload(result: IndexerResult) {
    const downloaderId = selectedDownloader || downloaders?.[0]?.id
    if (!downloaderId) {
      setErrorMessage('No downloader available')
      return
    }
    setDownloadingGuid(result.guid)
    const category = media.type === 'movie' ? 'movies' : 'tv'
    downloadMutation.mutate({
      downloadUrl: result.downloadUrl,
      downloaderId,
      releaseName: result.title,
      category,
    })
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={onClose}
      onKeyDown={(e) => e.key === 'Escape' && onClose()}
      role="dialog"
      aria-modal="true"
      aria-labelledby="release-modal-title"
    >
      <div
        className="relative w-full max-w-4xl max-h-[85vh] m-4 rounded-xl bg-card border border-border shadow-2xl flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <div>
            <h2 id="release-modal-title" className="text-lg font-semibold text-foreground">
              {media.title}
              {media.year && <span className="text-muted ml-2">({media.year})</span>}
            </h2>
            <p className="text-sm text-muted">
              {isSearching
                ? 'Searching indexers...'
                : `${indexerResults?.results.length || 0} releases found`}
            </p>
          </div>
          <div className="flex items-center gap-2">
            {canSwitchMode && onSwitchMode && (
              <button
                type="button"
                onClick={onSwitchMode}
                className="p-2 rounded-md text-muted hover:text-foreground hover:bg-background transition-colors"
                title="Switch to simple mode"
              >
                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h8m-8 6h16" />
                </svg>
              </button>
            )}
            <button
              type="button"
              onClick={onClose}
              className="p-2 rounded-md text-muted hover:text-foreground hover:bg-background transition-colors"
            >
              <svg
                className="w-5 h-5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
                aria-hidden="true"
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

        {/* Filters bar */}
        <div className="px-6 py-3 border-b border-border bg-background/50 flex items-center gap-4 flex-wrap">
          {/* Season selector for TV */}
          {media.type === 'tv' && (
            <div className="flex items-center gap-2">
              <span className="text-sm text-muted">Season:</span>
              <select
                value={selectedSeason ?? ''}
                onChange={(e) =>
                  setSelectedSeason(e.target.value ? Number(e.target.value) : undefined)
                }
                className="rounded-md bg-card border border-border px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-accent"
              >
                <option value="">All seasons</option>
                {tvDetails?.seasons
                  .filter((s) => s.seasonNumber > 0)
                  .map((s) => (
                    <option key={s.seasonNumber} value={s.seasonNumber}>
                      Season {s.seasonNumber} ({s.episodeCount} eps)
                    </option>
                  ))}
              </select>
              {tvDetails && (
                <span
                  className={`text-xs px-2 py-0.5 rounded ${
                    tvDetails.status === 'returning'
                      ? 'bg-green-500/10 text-green-400'
                      : tvDetails.status === 'ended'
                        ? 'bg-muted/20 text-muted'
                        : 'bg-yellow-500/10 text-yellow-400'
                  }`}
                >
                  {tvDetails.status === 'returning'
                    ? 'Ongoing'
                    : tvDetails.status === 'ended'
                      ? 'Ended'
                      : tvDetails.status === 'upcoming'
                        ? 'Upcoming'
                        : 'Unknown'}
                </span>
              )}
              {selectedSeason && tvDetails && (
                <button
                  type="button"
                  onClick={handleRequestSeason}
                  disabled={seasonRequestMutation.isPending}
                  className="rounded-md bg-accent px-4 py-1.5 text-sm font-medium text-white hover:bg-accent/90 transition-colors disabled:opacity-50"
                >
                  {seasonRequestMutation.isPending
                    ? 'Requesting...'
                    : `Request Season ${selectedSeason}`}
                </button>
              )}
            </div>
          )}

          {/* Quality filter */}
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted">Quality:</span>
            <div className="flex gap-1">
              {(['all', '2160p', '1080p', '720p'] as const).map((q) => (
                <button
                  key={q}
                  type="button"
                  onClick={() => setQualityFilter(q)}
                  className={`px-3 py-1 text-xs font-medium rounded-md transition-colors ${
                    qualityFilter === q
                      ? 'bg-accent text-white'
                      : 'bg-card text-muted hover:text-foreground border border-border'
                  }`}
                >
                  {q === 'all' ? 'All' : q === '2160p' ? '4K' : q.toUpperCase()}
                </button>
              ))}
            </div>
          </div>

          {/* Downloader selector */}
          {downloaders && downloaders.length > 1 && (
            <div className="flex items-center gap-2 ml-auto">
              <span className="text-sm text-muted">Send to:</span>
              <select
                value={selectedDownloader || downloaders[0]?.id}
                onChange={(e) => setSelectedDownloader(e.target.value)}
                className="rounded-md bg-card border border-border px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-accent"
              >
                {downloaders.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                  </option>
                ))}
              </select>
            </div>
          )}
        </div>

        {/* Messages */}
        {successMessage && (
          <div className="mx-6 mt-4 px-4 py-3 rounded-md bg-green-500/10 border border-green-500/20 text-green-400 text-sm">
            {successMessage}
          </div>
        )}
        {errorMessage && (
          <div className="mx-6 mt-4 px-4 py-3 rounded-md bg-red-500/10 border border-red-500/20 text-red-400 text-sm">
            {errorMessage}
          </div>
        )}

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6">
          {isSearching && (
            <div className="flex flex-col items-center justify-center py-12">
              <div className="h-10 w-10 animate-spin rounded-full border-2 border-accent border-t-transparent mb-4" />
              <p className="text-muted">Searching indexers...</p>
            </div>
          )}

          {searchError && (
            <div className="text-center py-12">
              <div className="w-16 h-16 rounded-full bg-red-500/10 flex items-center justify-center mx-auto mb-4">
                <svg
                  className="w-8 h-8 text-red-400"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                  aria-hidden="true"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={1.5}
                    d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                  />
                </svg>
              </div>
              <h3 className="text-lg font-medium text-foreground mb-1">Search Failed</h3>
              <p className="text-sm text-muted">{(searchError as Error).message}</p>
            </div>
          )}

          {!isSearching && !searchError && indexerResults?.results.length === 0 && (
            <div className="text-center py-12">
              <div className="w-16 h-16 rounded-full bg-card flex items-center justify-center mx-auto mb-4">
                <svg
                  className="w-8 h-8 text-muted"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                  aria-hidden="true"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={1.5}
                    d="M9.172 16.172a4 4 0 015.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                </svg>
              </div>
              <h3 className="text-lg font-medium text-foreground mb-1">No Releases Found</h3>
              <p className="text-sm text-muted mb-4">No releases were found across your indexers</p>

              {/* Add to requests option */}
              <div className="max-w-sm mx-auto p-4 rounded-lg bg-background border border-border">
                <p className="text-sm text-foreground mb-3">
                  Add to your requests and we'll keep searching automatically
                </p>
                {media.type === 'tv' && (
                  <div className="mb-3">
                    <label htmlFor="season-select" className="block text-xs text-muted mb-1">
                      Season (optional)
                    </label>
                    <select
                      id="season-select"
                      value={selectedSeason ?? ''}
                      onChange={(e) =>
                        setSelectedSeason(e.target.value ? Number(e.target.value) : undefined)
                      }
                      className="w-full rounded-md bg-card border border-border px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-accent"
                    >
                      <option value="">All seasons</option>
                      {tvDetails?.seasons
                        .filter((s) => s.seasonNumber > 0)
                        .map((s) => (
                          <option key={s.seasonNumber} value={s.seasonNumber}>
                            Season {s.seasonNumber} ({s.episodeCount} eps)
                          </option>
                        ))}
                    </select>
                  </div>
                )}
                <button
                  type="button"
                  onClick={handleAddToRequests}
                  disabled={requestMutation.isPending}
                  className="w-full rounded-md bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent/90 transition-colors disabled:opacity-50"
                >
                  {requestMutation.isPending ? 'Adding...' : 'Add to Requests'}
                </button>
              </div>

              {indexerResults?.errors && indexerResults.errors.length > 0 && (
                <div className="mt-4 text-left max-w-md mx-auto">
                  <p className="text-xs text-muted mb-2">Indexer errors:</p>
                  {indexerResults.errors.map((err) => (
                    <p key={err.indexer} className="text-xs text-red-400">
                      {err.indexer}: {err.error}
                    </p>
                  ))}
                </div>
              )}
            </div>
          )}

          {!isSearching &&
            indexerResults &&
            indexerResults.results.length > 0 &&
            (() => {
              const filteredResults = indexerResults.results.filter((r) =>
                matchesQualityFilter(r.parsed, qualityFilter)
              )
              return (
                <div className="space-y-2">
                  {/* Table header */}
                  <div className="grid grid-cols-[1fr_minmax(200px,auto)_100px_120px_80px_100px] gap-3 px-4 py-2 text-xs font-medium text-muted uppercase tracking-wider">
                    <div>Release</div>
                    <div>Quality</div>
                    <div>Size</div>
                    <div>Indexer</div>
                    <div>Grabs</div>
                    <div>Age</div>
                  </div>

                  {/* Results */}
                  {filteredResults.length === 0 ? (
                    <div className="text-center py-8">
                      <p className="text-muted">No releases match the selected quality filter</p>
                    </div>
                  ) : (
                    filteredResults.map((result) => {
                      const isExpanded = expandedGuid === result.guid
                      return (
                        <div
                          key={result.guid}
                          className="rounded-lg bg-background border border-border hover:border-accent/30 transition-colors"
                        >
                          <div className="grid grid-cols-[1fr_minmax(200px,auto)_100px_120px_80px_100px] gap-3 items-center px-4 py-3 group">
                            <div className="min-w-0">
                              <p className="text-sm text-foreground truncate" title={result.title}>
                                {result.title}
                              </p>
                            </div>
                            <div>
                              {result.parsed && (
                                <QualitySummary
                                  parsed={result.parsed}
                                  showExpandButton
                                  expanded={isExpanded}
                                  onToggleExpand={() =>
                                    setExpandedGuid(isExpanded ? null : result.guid)
                                  }
                                />
                              )}
                            </div>
                            <div className="text-sm text-muted">{formatBytes(result.size)}</div>
                            <div className="text-sm text-muted truncate" title={result.indexer}>
                              {result.indexer}
                            </div>
                            <div className="text-sm text-muted">{result.grabs ?? '-'}</div>
                            <div className="flex items-center justify-between gap-2">
                              <span className="text-sm text-muted">
                                {formatAge(result.pubDate)}
                              </span>
                              <button
                                type="button"
                                onClick={() => handleDownload(result)}
                                disabled={downloadingGuid === result.guid || !downloaders?.length}
                                className="opacity-0 group-hover:opacity-100 shrink-0 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent/90 transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                              >
                                {downloadingGuid === result.guid ? (
                                  <div className="h-4 w-4 animate-spin rounded-full border-2 border-white border-t-transparent" />
                                ) : (
                                  'Download'
                                )}
                              </button>
                            </div>
                          </div>
                          {isExpanded && result.parsed && (
                            <div className="px-4 pb-4">
                              <QualityDetails parsed={result.parsed} />
                            </div>
                          )}
                        </div>
                      )
                    })
                  )}

                  {/* Errors */}
                  {indexerResults.errors && indexerResults.errors.length > 0 && (
                    <div className="mt-4 px-4 py-3 rounded-lg bg-yellow-500/5 border border-yellow-500/20">
                      <p className="text-xs font-medium text-yellow-500 mb-1">
                        Some indexers had errors:
                      </p>
                      {indexerResults.errors.map((err) => (
                        <p key={err.indexer} className="text-xs text-yellow-400/80">
                          {err.indexer}: {err.error}
                        </p>
                      ))}
                    </div>
                  )}
                </div>
              )
            })()}
        </div>

        {/* Footer */}
        <div className="px-6 py-4 border-t border-border bg-background/50">
          {!downloaders?.length && !isSearching ? (
            <p className="text-sm text-yellow-500">
              No downloaders configured. Add a downloader in Settings to send downloads.
            </p>
          ) : (
            <div className="flex items-center justify-between">
              <p className="text-sm text-muted">
                Can't find what you need? Add to requests for automatic monitoring.
              </p>
              <button
                type="button"
                onClick={handleAddToRequests}
                disabled={requestMutation.isPending}
                className="text-sm text-accent hover:text-accent/80 font-medium transition-colors disabled:opacity-50"
              >
                {requestMutation.isPending ? 'Adding...' : 'Add to Requests'}
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
