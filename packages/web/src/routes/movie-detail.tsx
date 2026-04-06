import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useAuth } from '../lib/auth'
import { movieLanes, apiPost } from '../lib/api-client'
import { useRequestStatus } from '../hooks/useRequestStatus'
import { DownloadStatus } from '../components/DownloadStatus'
import { ReleaseModal } from './requests/ReleaseModal'

export default function MovieDetail({ tmdbId }: { tmdbId: string }) {
  const queryClient = useQueryClient()
  const { requestMode, canChooseRequestMode } = useAuth()
  const [quality, setQuality] = useState('1080p')
  const [showReleaseModal, setShowReleaseModal] = useState(false)

  const { data: movie, isLoading } = useQuery({
    queryKey: ['movie-details', tmdbId],
    queryFn: () => movieLanes.getById(tmdbId),
  })

  const { request, isLoading: isLoadingRequest } = useRequestStatus(tmdbId, 'movie')

  const requestMutation = useMutation({
    mutationFn: () =>
      apiPost<{ success: boolean }>('/api/requests', {
        title: movie?.title,
        year: movie?.year,
        type: 'movie',
        tmdbId,
        posterPath: movie?.poster,
        overview: movie?.overview,
        minQuality: quality === 'any' ? null : quality,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['request-status', tmdbId] })
      queryClient.invalidateQueries({ queryKey: ['requests'] })
    },
  })

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
      </div>
    )
  }

  if (!movie) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <p className="text-muted">Movie not found</p>
      </div>
    )
  }

  const backdropUrl = movie.backdrop
    ? `https://image.tmdb.org/t/p/original${movie.backdrop}`
    : undefined

  return (
    <div className="min-h-screen">
      {/* Hero backdrop */}
      <div className="relative h-[60vh] min-h-[400px] w-full">
        {backdropUrl ? (
          <img src={backdropUrl} alt="" className="absolute inset-0 w-full h-full object-cover" />
        ) : (
          <div className="absolute inset-0 bg-card" />
        )}
        <div className="absolute inset-0 bg-gradient-to-t from-background via-background/50 to-transparent" />
        <div className="absolute inset-0 bg-gradient-to-r from-background/80 via-transparent to-transparent" />
      </div>

      {/* Content */}
      <div className="relative -mt-48 px-6 pb-12 max-w-5xl mx-auto">
        <div className="flex flex-col md:flex-row gap-8">
          {/* Poster */}
          <div className="shrink-0">
            {movie.poster ? (
              <img
                src={`https://image.tmdb.org/t/p/w342${movie.poster}`}
                alt={movie.title}
                className="w-48 md:w-56 rounded-lg shadow-2xl"
              />
            ) : (
              <div className="w-48 md:w-56 aspect-[2/3] rounded-lg bg-card border border-border flex items-center justify-center">
                <svg className="w-12 h-12 text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z" />
                </svg>
              </div>
            )}
          </div>

          {/* Info */}
          <div className="flex-1 min-w-0">
            <h1 className="text-3xl md:text-4xl font-bold text-foreground mb-2">
              {movie.title}
            </h1>

            <div className="flex flex-wrap items-center gap-3 mb-4 text-sm text-muted">
              {movie.year && <span>{movie.year}</span>}
              {movie.runtime && <span>{movie.runtime} min</span>}
              {movie.rating !== undefined && movie.rating > 0 && (
                <span className="flex items-center gap-1">
                  <svg className="w-4 h-4 text-yellow-400" fill="currentColor" viewBox="0 0 20 20">
                    <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                  </svg>
                  {movie.rating.toFixed(1)}
                </span>
              )}
            </div>

            {movie.overview && (
              <p className="text-foreground/80 leading-relaxed mb-6 max-w-2xl">
                {movie.overview}
              </p>
            )}

            {/* Request / Status section */}
            <div className="space-y-4">
              {isLoadingRequest ? (
                <div className="h-10 w-32 animate-pulse rounded-md bg-card" />
              ) : request ? (
                <DownloadStatus status={request.status} />
              ) : (
                <div className="flex flex-wrap items-center gap-3">
                  <select
                    value={quality}
                    onChange={(e) => setQuality(e.target.value)}
                    className="rounded-md bg-card border border-border px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-accent"
                  >
                    <option value="2160p">4K (2160p)</option>
                    <option value="1080p">1080p</option>
                    <option value="720p">720p</option>
                    <option value="any">Any Quality</option>
                  </select>
                  <button
                    type="button"
                    onClick={() => requestMutation.mutate()}
                    disabled={requestMutation.isPending}
                    className="flex items-center gap-2 rounded-md bg-accent px-6 py-2 text-sm font-semibold text-white hover:bg-accent/90 transition-colors disabled:opacity-50"
                  >
                    {requestMutation.isPending ? (
                      <>
                        <div className="h-4 w-4 animate-spin rounded-full border-2 border-white border-t-transparent" />
                        Requesting...
                      </>
                    ) : (
                      'Request'
                    )}
                  </button>
                  {(requestMode === 'detailed' || canChooseRequestMode) && (
                    <button
                      type="button"
                      onClick={() => setShowReleaseModal(true)}
                      className="flex items-center gap-2 rounded-md bg-card border border-border px-5 py-2 text-sm font-medium text-foreground hover:bg-background transition-colors"
                    >
                      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M4 6h16M4 12h16M4 18h7" />
                      </svg>
                      Browse Releases
                    </button>
                  )}
                  {requestMutation.isError && (
                    <p className="text-sm text-red-400">{requestMutation.error.message}</p>
                  )}
                </div>
              )}
            </div>

            {/* External links */}
            <div className="flex items-center gap-3 mt-8">
              {movie.imdbId && (
                <a
                  href={`https://www.imdb.com/title/${movie.imdbId}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="px-3 py-1.5 rounded-md bg-card border border-border text-sm text-muted hover:text-foreground transition-colors"
                >
                  IMDb
                </a>
              )}
              <a
                href={`https://www.themoviedb.org/movie/${tmdbId}`}
                target="_blank"
                rel="noopener noreferrer"
                className="px-3 py-1.5 rounded-md bg-card border border-border text-sm text-muted hover:text-foreground transition-colors"
              >
                TMDB
              </a>
            </div>
          </div>
        </div>
      </div>

      {showReleaseModal && movie && (
        <ReleaseModal
          media={{
            id: tmdbId,
            title: movie.title,
            year: movie.year,
            overview: movie.overview,
            posterPath: movie.poster,
            type: 'movie',
          }}
          onClose={() => {
            setShowReleaseModal(false)
            queryClient.invalidateQueries({ queryKey: ['request-status', tmdbId] })
          }}
        />
      )}
    </div>
  )
}
