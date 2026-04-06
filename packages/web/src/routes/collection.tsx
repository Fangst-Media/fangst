import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useState } from 'react'
import { apiFetch } from '../lib/api-client'

type CollectionMovie = {
  tmdbId: number
  title: string
  overview?: string
  releaseDate?: string
  posterPath?: string
  downloaded: boolean
  file?: {
    id: string
    filename: string
    resolution?: string
    size?: number
  }
}

type CollectionData = {
  collection: {
    id: string
    tmdbCollectionId: number
    name: string
    overview?: string
    posterPath?: string
    backdropPath?: string
  }
  movies: CollectionMovie[]
  totalMovies: number
  downloadedMovies: number
}

async function getCollectionData(tmdbCollectionId: string): Promise<CollectionData> {
  return apiFetch(`/api/library/collections/${tmdbCollectionId}`)
}

function formatBytes(bytes: number): string {
  if (!bytes || bytes === 0) return '-'
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(0)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

export default function CollectionPage({ collectionId }: { collectionId: string }) {
  const [showMissing, setShowMissing] = useState(false)

  const { data, isLoading, error } = useQuery({
    queryKey: ['collection', collectionId],
    queryFn: () => getCollectionData(collectionId),
  })

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="text-center">
          <div className="mb-4 h-10 w-10 animate-spin rounded-full border-2 border-accent border-t-transparent mx-auto" />
          <p className="text-sm text-muted">Loading collection...</p>
        </div>
      </div>
    )
  }

  if (error || !data) {
    return (
      <div className="flex min-h-screen items-center justify-center p-4">
        <div className="rounded-lg bg-card border border-border p-6 max-w-md text-center">
          <div className="w-12 h-12 rounded-full bg-red-500/10 flex items-center justify-center mx-auto mb-4">
            <svg
              className="w-6 h-6 text-red-400"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
              />
            </svg>
          </div>
          <h2 className="text-lg font-semibold text-foreground mb-2">Failed to load collection</h2>
          <p className="text-sm text-muted">{error?.message || 'Unknown error'}</p>
          <Link
            to="/library"
            className="mt-4 inline-block text-sm text-accent hover:text-accent/80"
          >
            Back to library
          </Link>
        </div>
      </div>
    )
  }

  const { collection, movies, totalMovies, downloadedMovies } = data
  const backdropUrl = collection.backdropPath
    ? `https://image.tmdb.org/t/p/w1280${collection.backdropPath}`
    : null
  const posterUrl = collection.posterPath
    ? `https://image.tmdb.org/t/p/w342${collection.posterPath}`
    : '/placeholder-poster.png'

  const filteredMovies = showMissing ? movies.filter((m) => !m.downloaded) : movies

  return (
    <div className="min-h-screen">
      {/* Hero section with backdrop */}
      <div className="relative">
        {backdropUrl && (
          <>
            <div
              className="absolute inset-0 bg-cover bg-center"
              style={{ backgroundImage: `url(${backdropUrl})` }}
            />
            <div className="absolute inset-0 bg-gradient-to-t from-background via-background/80 to-background/40" />
          </>
        )}

        <div className="relative px-6 py-8 md:py-12">
          {/* Back button */}
          <Link
            to="/library"
            className="inline-flex items-center gap-2 text-sm text-muted hover:text-foreground mb-6"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M15 19l-7-7 7-7"
              />
            </svg>
            Back to Library
          </Link>

          <div className="flex gap-6">
            <img
              src={posterUrl}
              alt={collection.name}
              className="w-32 md:w-48 rounded-lg shadow-xl object-cover"
            />
            <div className="flex-1">
              <span className="inline-block px-2 py-0.5 text-xs rounded bg-purple-500/20 text-purple-400 mb-2">
                Collection
              </span>
              <h1 className="text-2xl md:text-3xl font-bold text-foreground mb-2">
                {collection.name}
              </h1>
              {collection.overview && (
                <p className="text-muted text-sm md:text-base max-w-2xl line-clamp-3 mb-4">
                  {collection.overview}
                </p>
              )}
              <div className="flex items-center gap-4">
                <div className="flex items-center gap-2">
                  <span
                    className={`text-lg font-semibold ${
                      downloadedMovies === totalMovies ? 'text-green-400' : 'text-accent'
                    }`}
                  >
                    {downloadedMovies}/{totalMovies}
                  </span>
                  <span className="text-sm text-muted">movies downloaded</span>
                </div>
                {downloadedMovies === totalMovies && (
                  <span className="px-2 py-0.5 text-xs rounded bg-green-500/20 text-green-400">
                    Complete
                  </span>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Filter bar */}
      <div className="px-6 py-4 border-b border-border">
        <div className="flex items-center gap-4">
          <button
            type="button"
            onClick={() => setShowMissing(false)}
            className={`px-3 py-1.5 text-sm rounded-md transition-colors ${
              !showMissing
                ? 'bg-accent text-white'
                : 'text-muted hover:text-foreground border border-border'
            }`}
          >
            All ({totalMovies})
          </button>
          <button
            type="button"
            onClick={() => setShowMissing(true)}
            className={`px-3 py-1.5 text-sm rounded-md transition-colors ${
              showMissing
                ? 'bg-accent text-white'
                : 'text-muted hover:text-foreground border border-border'
            }`}
          >
            Missing ({totalMovies - downloadedMovies})
          </button>
        </div>
      </div>

      {/* Movies grid */}
      <div className="p-6">
        <div className="grid gap-4">
          {filteredMovies.map((movie) => {
            const moviePosterUrl = movie.posterPath
              ? `https://image.tmdb.org/t/p/w92${movie.posterPath}`
              : '/placeholder-poster.png'
            const year = movie.releaseDate ? new Date(movie.releaseDate).getFullYear() : null

            return (
              <div
                key={movie.tmdbId}
                className={`flex gap-4 p-4 rounded-lg border ${
                  movie.downloaded
                    ? 'bg-card border-border'
                    : 'bg-background border-border/50 opacity-60'
                }`}
              >
                <img
                  src={moviePosterUrl}
                  alt={movie.title}
                  className="w-16 h-24 rounded object-cover shrink-0"
                />
                <div className="flex-1 min-w-0">
                  <div className="flex items-start justify-between gap-4">
                    <div>
                      <h3 className="font-medium text-foreground">
                        {movie.title}
                        {year && <span className="text-muted ml-2">({year})</span>}
                      </h3>
                      {movie.overview && (
                        <p className="text-sm text-muted line-clamp-2 mt-1">{movie.overview}</p>
                      )}
                    </div>
                    {movie.downloaded ? (
                      <span className="shrink-0 px-2 py-0.5 text-xs rounded bg-green-500/20 text-green-400">
                        Downloaded
                      </span>
                    ) : (
                      <span className="shrink-0 px-2 py-0.5 text-xs rounded bg-muted/20 text-muted">
                        Missing
                      </span>
                    )}
                  </div>
                  {movie.file && (
                    <div className="mt-2 flex items-center gap-3 text-xs text-muted">
                      {movie.file.resolution && (
                        <span className="px-1.5 py-0.5 rounded bg-black/30">
                          {movie.file.resolution}
                        </span>
                      )}
                      {movie.file.size && <span>{formatBytes(movie.file.size)}</span>}
                      <span className="truncate">{movie.file.filename}</span>
                    </div>
                  )}
                </div>
              </div>
            )
          })}
        </div>

        {filteredMovies.length === 0 && (
          <div className="text-center py-12">
            <p className="text-muted">
              {showMissing ? 'All movies in this collection are downloaded!' : 'No movies found'}
            </p>
          </div>
        )}
      </div>
    </div>
  )
}
