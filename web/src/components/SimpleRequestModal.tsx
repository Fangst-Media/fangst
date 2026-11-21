import type { TvSeriesDetails } from '@fangst/shared/schemas/metadata'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { apiPost, collections, movieLanes, tvLanes } from '../lib/api-client'
import { type MonitoringPreset, MonitoringPresetSelector } from './MonitoringPresetSelector'

type SearchResult = {
  id: string
  title: string
  year?: number
  releaseDate?: string
  overview?: string
  posterPath?: string
  type: 'movie' | 'tv'
}

type SimpleRequestModalProps = {
  media: SearchResult
  onClose: () => void
  defaultQuality?: string
  canSwitchMode?: boolean
  onSwitchMode?: () => void
}

async function createRequest(data: {
  title: string
  year?: number
  type: 'movie' | 'tv'
  tmdbId?: string
  posterPath?: string
  overview?: string
  minQuality?: string | null
  season?: number
  seriesStatus?: TvSeriesDetails['status']
  fetchEpisodes?: boolean
  releaseDate?: string
  monitoringPreset?: MonitoringPreset
}): Promise<{ success: boolean }> {
  return apiPost<{ success: boolean }>('/api/requests', data)
}

export function SimpleRequestModal({
  media,
  onClose,
  defaultQuality = '1080p',
  canSwitchMode,
  onSwitchMode,
}: SimpleRequestModalProps) {
  const [quality, setQuality] = useState(defaultQuality)
  const [selectedSeason, setSelectedSeason] = useState<number | 'all' | undefined>(undefined)
  const [monitoringPreset, setMonitoringPreset] = useState<MonitoringPreset>('all')
  const [successMessage, setSuccessMessage] = useState<string | null>(null)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [showCollectionPrompt, setShowCollectionPrompt] = useState(false)
  const queryClient = useQueryClient()

  const { data: tvDetails, isLoading: isLoadingTvDetails } = useQuery({
    queryKey: ['tv-details', media.id],
    queryFn: () => tvLanes.getById(media.id),
    enabled: media.type === 'tv',
  })

  const { data: movieDetails } = useQuery({
    queryKey: ['movie-details', media.id],
    queryFn: () => movieLanes.getById(media.id),
    enabled: media.type === 'movie',
  })

  const { data: collectionDetails } = useQuery({
    queryKey: ['collection-details', movieDetails?.collection?.id],
    queryFn: () => collections.getById(movieDetails!.collection!.id.toString()),
    enabled: !!movieDetails?.collection?.id,
  })

  const requestMutation = useMutation({
    mutationFn: async () => {
      return createRequest({
        title: media.title,
        year: media.year,
        type: media.type,
        tmdbId: media.id,
        posterPath: media.posterPath,
        overview: media.overview,
        minQuality: quality === 'any' ? null : quality,
        ...(media.type === 'movie' && {
          releaseDate: media.releaseDate,
        }),
        ...(media.type === 'tv' && {
          season: selectedSeason === 'all' ? undefined : selectedSeason,
          seriesStatus: tvDetails?.status,
          fetchEpisodes: true,
          monitoringPreset,
        }),
      })
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['requests'] })
      setSuccessMessage('Request created!')
      setTimeout(() => {
        setSuccessMessage(null)
        onClose()
      }, 1500)
    },
    onError: (error: Error) => {
      setErrorMessage(error.message)
      setTimeout(() => setErrorMessage(null), 5000)
    },
  })

  function getPosterUrl(path?: string) {
    if (!path) return '/placeholder-poster.png'
    return `https://image.tmdb.org/t/p/w185${path}`
  }

  function handleRequest() {
    if (media.type === 'movie' && movieDetails?.collection && !showCollectionPrompt) {
      setShowCollectionPrompt(true)
      return
    }
    requestMutation.mutate()
  }

  function handleRequestJustThisMovie() {
    setShowCollectionPrompt(false)
    requestMutation.mutate()
  }

  async function handleRequestCollection() {
    if (!collectionDetails) return

    setShowCollectionPrompt(false)

    for (const movie of collectionDetails.movies) {
      await createRequest({
        title: movie.title,
        year: movie.releaseDate ? new Date(movie.releaseDate).getFullYear() : undefined,
        type: 'movie',
        tmdbId: movie.id.toString(),
        posterPath: movie.posterPath,
        overview: movie.overview,
        minQuality: quality === 'any' ? null : quality,
      })
    }

    queryClient.invalidateQueries({ queryKey: ['requests'] })
    setSuccessMessage(
      `Added ${collectionDetails.movies.length} movies from "${collectionDetails.name}"!`
    )
    setTimeout(() => {
      setSuccessMessage(null)
      onClose()
    }, 2000)
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={onClose}
      onKeyDown={(e) => e.key === 'Escape' && onClose()}
      // biome-ignore lint/a11y/useSemanticElements: a native <dialog> only blocks the page through showModal(); this overlay is rendered conditionally instead
      role="dialog"
      aria-modal="true"
      aria-labelledby="simple-request-title"
    >
      <div
        className="relative w-full max-w-sm m-4 rounded-xl bg-card border border-border shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        {/* Header with poster */}
        <div className="flex gap-4 p-4 border-b border-border">
          <img
            src={getPosterUrl(media.posterPath)}
            alt={media.title}
            className="w-16 h-24 rounded object-cover bg-background"
          />
          <div className="flex-1 min-w-0">
            <h2
              id="simple-request-title"
              className="font-semibold text-foreground truncate"
              title={`${media.title}${media.year ? ` (${media.year})` : ''}`}
            >
              {media.title}
            </h2>
            {media.year && <p className="text-sm text-muted mt-0.5">({media.year})</p>}
            <span className="inline-block mt-2 px-2 py-0.5 text-xs rounded bg-accent/10 text-accent uppercase">
              {media.type}
            </span>
          </div>
          {canSwitchMode && onSwitchMode && (
            <button
              type="button"
              onClick={onSwitchMode}
              className="self-start p-1.5 rounded-md text-muted hover:text-foreground hover:bg-background transition-colors"
              title="Switch to detailed mode"
            >
              <svg
                aria-hidden="true"
                className="w-5 h-5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M4 6h16M4 12h16M4 18h7"
                />
              </svg>
            </button>
          )}
        </div>

        {/* Messages */}
        {successMessage && (
          <div className="mx-4 mt-4 px-4 py-3 rounded-md bg-green-500/10 border border-green-500/20 text-green-400 text-sm">
            {successMessage}
          </div>
        )}
        {errorMessage && (
          <div className="mx-4 mt-4 px-4 py-3 rounded-md bg-red-500/10 border border-red-500/20 text-red-400 text-sm">
            {errorMessage}
          </div>
        )}

        {/* Season selector for TV shows */}
        {media.type === 'tv' && (
          <div className="p-4 pb-0">
            <label
              htmlFor="season-select"
              className="block text-sm font-medium text-foreground mb-2"
            >
              Season
            </label>
            {isLoadingTvDetails ? (
              <div className="flex items-center gap-2 text-sm text-muted py-2">
                <div className="h-4 w-4 animate-spin rounded-full border-2 border-accent border-t-transparent" />
                Loading seasons...
              </div>
            ) : (
              <select
                id="season-select"
                value={selectedSeason ?? 'all'}
                onChange={(e) =>
                  setSelectedSeason(e.target.value === 'all' ? 'all' : Number(e.target.value))
                }
                className="w-full rounded-md bg-background border border-border px-3 py-2.5 text-foreground focus:outline-none focus:ring-2 focus:ring-accent"
              >
                <option value="all">All Seasons</option>
                {tvDetails?.seasons
                  .filter((s) => s.seasonNumber > 0)
                  .map((season) => (
                    <option key={season.seasonNumber} value={season.seasonNumber}>
                      {season.name} ({season.episodeCount} episodes)
                    </option>
                  ))}
              </select>
            )}
            <p className="mt-2 text-xs text-muted">
              {selectedSeason === 'all' || selectedSeason === undefined
                ? 'All available seasons will be requested.'
                : 'Only this season will be requested.'}
            </p>
            <div className="mt-4">
              <MonitoringPresetSelector
                value={monitoringPreset}
                onChange={setMonitoringPreset}
                seriesStatus={tvDetails?.status}
              />
            </div>
          </div>
        )}

        {/* Quality selector */}
        <div className="p-4">
          <label
            htmlFor="quality-select"
            className="block text-sm font-medium text-foreground mb-2"
          >
            Preferred Quality
          </label>
          <select
            id="quality-select"
            value={quality}
            onChange={(e) => setQuality(e.target.value)}
            className="w-full rounded-md bg-background border border-border px-3 py-2.5 text-foreground focus:outline-none focus:ring-2 focus:ring-accent"
          >
            <option value="2160p">4K (2160p)</option>
            <option value="1080p">1080p</option>
            <option value="720p">720p</option>
            <option value="any">Any Quality</option>
          </select>
          <p className="mt-2 text-xs text-muted">
            We'll find the best available release at or above this quality.
          </p>
        </div>

        {/* Collection Prompt */}
        {showCollectionPrompt && movieDetails?.collection && (
          <div className="mx-4 mb-4 p-4 rounded-lg bg-background border border-accent/30">
            <p className="text-sm text-foreground mb-2">
              <span className="font-medium">{media.title}</span> is part of{' '}
              <span className="text-accent font-medium">{movieDetails.collection.name}</span>
            </p>

            {collectionDetails && (
              <div className="mb-3">
                <p className="text-xs text-muted mb-1.5">
                  {collectionDetails.movies.length} movies in collection:
                </p>
                <div className="flex flex-wrap gap-1.5">
                  {collectionDetails.movies.map((movie) => (
                    <span
                      key={movie.id}
                      className={`text-xs px-1.5 py-0.5 rounded ${
                        movie.id.toString() === media.id
                          ? 'bg-accent/20 text-accent'
                          : 'bg-card text-muted'
                      }`}
                    >
                      {movie.title}
                    </span>
                  ))}
                </div>
              </div>
            )}

            <div className="flex gap-2">
              <button
                type="button"
                onClick={handleRequestJustThisMovie}
                disabled={requestMutation.isPending}
                className="flex-1 px-3 py-2 rounded-md bg-card border border-border text-sm font-medium text-foreground hover:bg-background transition-colors disabled:opacity-50"
              >
                Just this movie
              </button>
              <button
                type="button"
                onClick={handleRequestCollection}
                disabled={!collectionDetails || requestMutation.isPending}
                className="flex-1 px-3 py-2 rounded-md bg-accent text-sm font-medium text-white hover:bg-accent/90 transition-colors disabled:opacity-50"
              >
                {!collectionDetails ? 'Loading...' : `All ${collectionDetails.movies.length}`}
              </button>
            </div>
          </div>
        )}

        {/* Actions */}
        {!showCollectionPrompt && (
          <div className="flex gap-3 p-4 pt-0">
            <button
              type="button"
              onClick={onClose}
              className="flex-1 rounded-md px-4 py-2.5 text-sm font-medium text-muted border border-border hover:text-foreground hover:border-foreground/30 transition-colors"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleRequest}
              disabled={requestMutation.isPending || !!successMessage}
              className="flex-1 rounded-md bg-accent px-4 py-2.5 text-sm font-medium text-white hover:bg-accent/90 transition-colors disabled:opacity-50"
            >
              {requestMutation.isPending ? (
                <span className="flex items-center justify-center gap-2">
                  <div className="h-4 w-4 animate-spin rounded-full border-2 border-white border-t-transparent" />
                  Requesting...
                </span>
              ) : (
                'Request'
              )}
            </button>
          </div>
        )}
      </div>
    </div>
  )
}
