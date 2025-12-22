import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState, useRef } from 'react'
import type { ParsedReleaseInfo } from '@jaeger/shared/types/indexer'
import { QualitySummary, QualityDetails } from '../components/quality'
import { SimpleRequestModal } from '../components/SimpleRequestModal'
import { useAuth } from '../lib/auth'

type TrendingItem = {
  id: number
  title?: string
  name?: string
  poster_path: string | null
  release_date?: string
  first_air_date?: string
  overview: string | null
  vote_average: number
  media_type: 'movie' | 'tv'
}

async function fetchTrendingMovies(): Promise<TrendingItem[]> {
  const response = await fetch('/api/metadata/tmdb/movies/trending')
  if (!response.ok) return []
  const data = await response.json()
  return (data.results || []).map((item: TrendingItem) => ({ ...item, media_type: 'movie' }))
}

async function fetchTrendingTv(): Promise<TrendingItem[]> {
  const response = await fetch('/api/metadata/tmdb/tv/trending')
  if (!response.ok) return []
  const data = await response.json()
  return (data.results || []).map((item: TrendingItem) => ({ ...item, media_type: 'tv' }))
}

function TrendingLane({
  title,
  items,
  onSelect,
}: {
  title: string
  items: TrendingItem[]
  onSelect: (item: TrendingItem) => void
}) {
  const scrollRef = useRef<HTMLDivElement>(null)

  const scroll = (direction: 'left' | 'right') => {
    if (scrollRef.current) {
      const scrollAmount = scrollRef.current.clientWidth * 0.75
      scrollRef.current.scrollBy({
        left: direction === 'left' ? -scrollAmount : scrollAmount,
        behavior: 'smooth',
      })
    }
  }

  if (items.length === 0) return null

  return (
    <div className="mb-8">
      <h2 className="text-lg font-semibold text-foreground mb-3">{title}</h2>
      <div className="relative group">
        <button
          type="button"
          onClick={() => scroll('left')}
          className="absolute left-0 top-1/2 -translate-y-1/2 z-10 w-10 h-10 rounded-full bg-background/80 border border-border flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity hover:bg-card"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <div
          ref={scrollRef}
          className="flex gap-3 overflow-x-auto scrollbar-hide pb-2"
          style={{ scrollbarWidth: 'none', msOverflowStyle: 'none' }}
        >
          {items.slice(0, 20).map((item) => {
            const itemTitle = item.title || item.name || 'Unknown'
            const year = item.release_date || item.first_air_date
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => onSelect(item)}
                className="flex-shrink-0 w-32 group/card text-left"
              >
                <div className="relative aspect-[2/3] rounded-lg overflow-hidden bg-card border border-border group-hover/card:border-accent/50 transition-colors">
                  {item.poster_path ? (
                    <img
                      src={`https://image.tmdb.org/t/p/w185${item.poster_path}`}
                      alt={itemTitle}
                      className="w-full h-full object-cover"
                      loading="lazy"
                    />
                  ) : (
                    <div className="w-full h-full flex items-center justify-center bg-card">
                      <svg className="w-8 h-8 text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z" />
                      </svg>
                    </div>
                  )}
                  <div className="absolute bottom-0 left-0 right-0 p-2 bg-gradient-to-t from-black/80 to-transparent opacity-0 group-hover/card:opacity-100 transition-opacity">
                    <span className="text-xs text-white font-medium">Request</span>
                  </div>
                </div>
                <p className="mt-2 text-sm text-foreground truncate">{itemTitle}</p>
                {year && (
                  <p className="text-xs text-muted">{new Date(year).getFullYear()}</p>
                )}
              </button>
            )
          })}
        </div>
        <button
          type="button"
          onClick={() => scroll('right')}
          className="absolute right-0 top-1/2 -translate-y-1/2 z-10 w-10 h-10 rounded-full bg-background/80 border border-border flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity hover:bg-card"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </div>
    </div>
  )
}

type SearchResult = {
  id: string
  title: string
  year?: number
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

async function searchContent(query: string): Promise<SearchResult[]> {
  if (!query.trim()) return []

  const [movies, tv] = await Promise.all([
    fetch(
      `/api/metadata/tmdb/movies/search?query=${encodeURIComponent(query)}`
    ).then((r) => r.json()),
    fetch(
      `/api/metadata/tmdb/tv/search?query=${encodeURIComponent(query)}`
    ).then((r) => r.json()),
  ])

  const movieResults: SearchResult[] =
    movies.results?.slice(0, 5).map(
      (m: {
        id: number
        title: string
        release_date?: string
        overview?: string
        poster_path?: string
      }) => ({
        id: m.id.toString(),
        title: m.title,
        year: m.release_date ? new Date(m.release_date).getFullYear() : undefined,
        overview: m.overview,
        posterPath: m.poster_path,
        type: 'movie' as const,
      })
    ) || []

  const tvResults: SearchResult[] =
    tv.results?.slice(0, 5).map(
      (t: {
        id: number
        name: string
        first_air_date?: string
        overview?: string
        poster_path?: string
      }) => ({
        id: t.id.toString(),
        title: t.name,
        year: t.first_air_date ? new Date(t.first_air_date).getFullYear() : undefined,
        overview: t.overview,
        posterPath: t.poster_path,
        type: 'tv' as const,
      })
    ) || []

  return [...movieResults, ...tvResults]
}

async function getMovieDetails(tmdbId: string): Promise<{ imdbId?: string; runtime?: number }> {
  const response = await fetch(`/api/metadata/tmdb/movies/${tmdbId}`)
  if (!response.ok) throw new Error('Failed to fetch movie details')
  const data = await response.json()
  return { imdbId: data.imdb_id, runtime: data.runtime ?? undefined }
}

async function searchIndexers(params: {
  imdbId?: string
  query?: string
  type: 'movie' | 'tv'
  runtime?: number
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

function formatBytes(bytes: number): string {
  if (!bytes || bytes === 0) return '-'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

type QualityFilter = '2160p' | '1080p' | '720p' | 'all'

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

function ReleaseModal({
  media,
  onClose,
}: {
  media: SearchResult
  onClose: () => void
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

  const {
    data: indexerResults,
    isLoading: isSearching,
    error: searchError,
  } = useQuery({
    queryKey: ['indexer-search', media.id, media.type],
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
      return searchIndexers({ query, type: 'tv' })
    },
  })

  const requestMutation = useMutation({
    mutationFn: createRequest,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['requests'] })
      setSuccessMessage('Added to requests! We\'ll keep searching.')
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

  const downloadMutation = useMutation({
    mutationFn: async ({ downloadUrl, downloaderId, releaseName, category }: { downloadUrl: string; downloaderId: string; releaseName: string; category?: string }) => {
      // First create a request record to track this download
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
      // Then send to downloader
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
    downloadMutation.mutate({ downloadUrl: result.downloadUrl, downloaderId, releaseName: result.title, category })
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

        {/* Filters bar */}
        <div className="px-6 py-3 border-b border-border bg-background/50 flex items-center gap-4 flex-wrap">
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
                      onChange={(e) => setSelectedSeason(e.target.value ? Number(e.target.value) : undefined)}
                      className="w-full rounded-md bg-card border border-border px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-accent"
                    >
                      <option value="">All seasons</option>
                      {[1, 2, 3, 4, 5, 6, 7, 8, 9, 10].map((s) => (
                        <option key={s} value={s}>Season {s}</option>
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

          {!isSearching && indexerResults && indexerResults.results.length > 0 && (() => {
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
                    <div key={result.guid} className="rounded-lg bg-background border border-border hover:border-accent/30 transition-colors">
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
                              onToggleExpand={() => setExpandedGuid(isExpanded ? null : result.guid)}
                            />
                          )}
                        </div>
                        <div className="text-sm text-muted">{formatBytes(result.size)}</div>
                        <div className="text-sm text-muted truncate" title={result.indexer}>
                          {result.indexer}
                        </div>
                        <div className="text-sm text-muted">{result.grabs ?? '-'}</div>
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-sm text-muted">{formatAge(result.pubDate)}</span>
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
          )})()}
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

export default function Requests() {
  const { requestMode, canChooseRequestMode, setRequestMode } = useAuth()
  const [query, setQuery] = useState('')
  const [debouncedQuery, setDebouncedQuery] = useState('')
  const [selectedMedia, setSelectedMedia] = useState<SearchResult | null>(null)

  useEffect(() => {
    const timeout = setTimeout(() => setDebouncedQuery(query), 400)
    return () => clearTimeout(timeout)
  }, [query])

  const { data: results, isLoading } = useQuery({
    queryKey: ['search', debouncedQuery],
    queryFn: () => searchContent(debouncedQuery),
    enabled: debouncedQuery.length > 2,
  })

  const { data: trendingMovies = [] } = useQuery({
    queryKey: ['trending-movies'],
    queryFn: fetchTrendingMovies,
    staleTime: 1000 * 60 * 10,
  })

  const { data: trendingTv = [] } = useQuery({
    queryKey: ['trending-tv'],
    queryFn: fetchTrendingTv,
    staleTime: 1000 * 60 * 10,
  })

  function getPosterUrl(path?: string) {
    if (!path) return '/placeholder-poster.png'
    return `https://image.tmdb.org/t/p/w185${path}`
  }

  function handleRequest(result: SearchResult) {
    setSelectedMedia(result)
  }

  function handleTrendingSelect(item: TrendingItem) {
    const year = item.release_date || item.first_air_date
    setSelectedMedia({
      id: item.id.toString(),
      title: item.title || item.name || 'Unknown',
      year: year ? new Date(year).getFullYear() : undefined,
      overview: item.overview ?? undefined,
      posterPath: item.poster_path ?? undefined,
      type: item.media_type,
    })
  }

  return (
    <div className="min-h-screen p-6">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Discover</h1>
          <p className="text-sm text-muted">Search and request new content</p>
        </div>

        {canChooseRequestMode && (
          <button
            type="button"
            onClick={() => setRequestMode(requestMode === 'simple' ? 'detailed' : 'simple')}
            className="flex items-center gap-2 px-3 py-1.5 rounded-md text-sm text-muted hover:text-foreground border border-border hover:border-foreground/30 transition-colors"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
            {requestMode === 'simple' ? 'Simple Mode' : 'Detailed Mode'}
          </button>
        )}
      </div>

      <div>
        <div className="max-w-2xl mb-8">
          <label htmlFor="search" className="sr-only">
            Search for movies or TV shows
          </label>
          <div className="relative">
            <input
              id="search"
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search for movies or TV shows..."
              className="w-full rounded-lg bg-card border border-border px-4 py-3 pl-12 text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent"
            />
            <svg
              className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-muted"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
              aria-hidden="true"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
              />
            </svg>
          </div>
        </div>

        {isLoading && (
          <div className="flex justify-center py-12">
            <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
          </div>
        )}

        {results && results.length > 0 && (
          <div className="max-w-4xl space-y-4">
            {results.map((result) => (
              <div
                key={`${result.type}-${result.id}`}
                className="flex gap-4 p-4 rounded-lg bg-card border border-border hover:border-accent/50 transition-colors"
              >
                <img
                  src={getPosterUrl(result.posterPath)}
                  alt={result.title}
                  className="w-16 h-24 rounded object-cover bg-background"
                />
                <div className="flex-1 min-w-0">
                  <div className="flex items-start justify-between gap-4">
                    <div>
                      <h3 className="font-medium text-foreground">
                        {result.title}
                        {result.year && <span className="text-muted ml-2">({result.year})</span>}
                      </h3>
                      <span className="inline-block mt-1 px-2 py-0.5 text-xs rounded bg-accent/10 text-accent uppercase">
                        {result.type}
                      </span>
                    </div>
                    <button
                      type="button"
                      onClick={() => handleRequest(result)}
                      className="shrink-0 rounded-md bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent/90 transition-colors"
                    >
                      Request
                    </button>
                  </div>
                  {result.overview && (
                    <p className="mt-2 text-sm text-muted line-clamp-2">{result.overview}</p>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}

        {debouncedQuery.length > 2 && !isLoading && results?.length === 0 && (
          <div className="text-center py-12">
            <p className="text-muted">No results found for "{debouncedQuery}"</p>
          </div>
        )}

        {!debouncedQuery && (
          <div className="mt-8">
            <TrendingLane
              title="Trending Movies"
              items={trendingMovies}
              onSelect={handleTrendingSelect}
            />
            <TrendingLane
              title="Trending TV Shows"
              items={trendingTv}
              onSelect={handleTrendingSelect}
            />
          </div>
        )}
      </div>

      {/* Modal - conditional based on request mode */}
      {selectedMedia && (
        requestMode === 'simple' ? (
          <SimpleRequestModal media={selectedMedia} onClose={() => setSelectedMedia(null)} />
        ) : (
          <ReleaseModal media={selectedMedia} onClose={() => setSelectedMedia(null)} />
        )
      )}
    </div>
  )
}
