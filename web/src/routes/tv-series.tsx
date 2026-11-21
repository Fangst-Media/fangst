import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useState } from 'react'
import { apiFetch } from '../lib/api-client'

type SeasonProgress = {
  seasonNumber: number
  name: string
  episodeCount: number
  downloadedCount: number
  airDate?: string
}

type SeriesWithSeasons = {
  series: {
    tmdbId: string
    title: string
    year?: number
    overview?: string
    posterPath?: string
    backdropPath?: string
    rating?: number
    status: 'returning' | 'ended' | 'upcoming' | 'unknown'
    numberOfSeasons: number
  }
  seasons: SeasonProgress[]
  totalDownloaded: number
  totalEpisodes: number
}

async function getTvSeries(tmdbId: string): Promise<SeriesWithSeasons> {
  return apiFetch(`/api/library/tv/${tmdbId}`)
}

function getPosterUrl(path?: string): string {
  if (!path) return '/placeholder-poster.png'
  return `https://image.tmdb.org/t/p/w342${path}`
}

function getBackdropUrl(path?: string): string {
  if (!path) return ''
  return `https://image.tmdb.org/t/p/w1280${path}`
}

export default function TvSeriesPage({ tmdbId }: { tmdbId: string }) {
  const [imgError, setImgError] = useState(false)

  const { data, isLoading, error } = useQuery({
    queryKey: ['library-tv-series', tmdbId],
    queryFn: () => getTvSeries(tmdbId),
  })

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="text-center">
          <div className="mb-4 h-10 w-10 animate-spin rounded-full border-2 border-accent border-t-transparent mx-auto" />
          <p className="text-sm text-muted">Loading series...</p>
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
              aria-hidden="true"
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
          <h2 className="text-lg font-semibold text-foreground mb-2">Series not found</h2>
          <p className="text-sm text-muted mb-4">
            {error instanceof Error ? error.message : 'Could not load this series'}
          </p>
          <Link to="/library" className="text-accent hover:text-accent/80 text-sm font-medium">
            Back to Library
          </Link>
        </div>
      </div>
    )
  }

  const { series, seasons, totalDownloaded, totalEpisodes } = data
  const progressPercent = totalEpisodes > 0 ? (totalDownloaded / totalEpisodes) * 100 : 0
  const hasBackdrop = series.backdropPath && !imgError

  return (
    <div className="min-h-screen">
      {/* Hero section with backdrop */}
      <div className="-mt-16 relative">
        {hasBackdrop && (
          <div
            className="absolute inset-0 h-[400px] bg-cover bg-center"
            style={{ backgroundImage: `url(${getBackdropUrl(series.backdropPath)})` }}
          >
            <div className="absolute inset-0 bg-gradient-to-t from-background via-background/80 to-background/40" />
          </div>
        )}

        <div className={`relative ${hasBackdrop ? 'pt-32' : 'pt-22'} px-6 pb-6`}>
          {/* Breadcrumb */}
          <nav className="flex items-center gap-2 text-sm mb-6">
            <Link to="/library" className="text-muted hover:text-foreground transition-colors">
              Library
            </Link>
            <span className="text-muted">/</span>
            <span className="text-foreground">{series.title}</span>
          </nav>

          {/* Series info */}
          <div className="flex gap-6">
            {/* Poster */}
            <div className="shrink-0">
              <img
                src={getPosterUrl(series.posterPath)}
                alt={series.title}
                className="w-48 h-72 rounded-lg object-cover shadow-xl"
                onError={() => setImgError(true)}
              />
            </div>

            {/* Details */}
            <div className="flex-1 min-w-0">
              <div className="flex items-start gap-4 mb-4">
                <div className="flex-1">
                  <h1 className="text-3xl font-bold text-foreground mb-2">{series.title}</h1>
                  <div className="flex items-center gap-3 text-sm">
                    {series.year && <span className="text-muted">{series.year}</span>}
                    <span
                      className={`px-2 py-0.5 rounded text-xs font-medium ${
                        series.status === 'returning'
                          ? 'bg-green-500/10 text-green-400'
                          : series.status === 'ended'
                            ? 'bg-muted/20 text-muted'
                            : 'bg-yellow-500/10 text-yellow-400'
                      }`}
                    >
                      {series.status === 'returning'
                        ? 'Continuing'
                        : series.status === 'ended'
                          ? 'Ended'
                          : series.status === 'upcoming'
                            ? 'Upcoming'
                            : 'Unknown'}
                    </span>
                    {series.rating && (
                      <span className="flex items-center gap-1 text-muted">
                        <span className="text-yellow-400">★</span>
                        {series.rating.toFixed(1)}
                      </span>
                    )}
                    <span className="text-muted">{series.numberOfSeasons} seasons</span>
                  </div>
                </div>
              </div>

              {series.overview && (
                <p className="text-muted text-sm leading-relaxed mb-6 max-w-2xl line-clamp-3">
                  {series.overview}
                </p>
              )}

              {/* Progress bar */}
              <div className="max-w-md">
                <div className="flex items-center justify-between text-sm mb-2">
                  <span className="text-muted">Collection Progress</span>
                  <span className="text-foreground font-medium">
                    {totalDownloaded} / {totalEpisodes} episodes
                  </span>
                </div>
                <div className="h-2 bg-card rounded-full overflow-hidden">
                  <div
                    className="h-full bg-accent transition-all duration-300"
                    style={{ width: `${progressPercent}%` }}
                  />
                </div>
                <p className="text-xs text-muted mt-1">{progressPercent.toFixed(0)}% complete</p>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Seasons grid */}
      <div className="px-6 pb-6">
        <h2 className="text-lg font-semibold text-foreground mb-4">Seasons</h2>
        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4">
          {seasons
            .filter((s) => s.seasonNumber > 0)
            .map((season) => {
              const seasonProgress =
                season.episodeCount > 0 ? (season.downloadedCount / season.episodeCount) * 100 : 0

              return (
                <Link
                  key={season.seasonNumber}
                  to="/library/tv/$tmdbId/season/$seasonNumber"
                  params={{ tmdbId, seasonNumber: String(season.seasonNumber) }}
                  className="group block"
                >
                  <div className="rounded-lg bg-card border border-border p-4 hover:border-accent/50 transition-colors">
                    <div className="flex items-center justify-between mb-3">
                      <h3 className="font-medium text-foreground group-hover:text-accent transition-colors">
                        {season.name}
                      </h3>
                      <svg
                        aria-hidden="true"
                        className="w-4 h-4 text-muted group-hover:text-accent transition-colors"
                        fill="none"
                        stroke="currentColor"
                        viewBox="0 0 24 24"
                      >
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          strokeWidth={2}
                          d="M9 5l7 7-7 7"
                        />
                      </svg>
                    </div>

                    <div className="space-y-2">
                      <div className="flex items-center justify-between text-sm">
                        <span className="text-muted">Episodes</span>
                        <span className="text-foreground">
                          {season.downloadedCount} / {season.episodeCount}
                        </span>
                      </div>

                      <div className="h-1.5 bg-background rounded-full overflow-hidden">
                        <div
                          className={`h-full transition-all duration-300 ${
                            seasonProgress === 100
                              ? 'bg-green-500'
                              : seasonProgress > 0
                                ? 'bg-accent'
                                : 'bg-muted/30'
                          }`}
                          style={{ width: `${seasonProgress}%` }}
                        />
                      </div>

                      {season.airDate && (
                        <p className="text-xs text-muted">
                          Aired {new Date(season.airDate).toLocaleDateString()}
                        </p>
                      )}
                    </div>
                  </div>
                </Link>
              )
            })}
        </div>

        {/* Specials if any */}
        {seasons.some((s) => s.seasonNumber === 0) && (
          <div className="mt-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Specials</h2>
            {seasons
              .filter((s) => s.seasonNumber === 0)
              .map((season) => {
                const seasonProgress =
                  season.episodeCount > 0 ? (season.downloadedCount / season.episodeCount) * 100 : 0

                return (
                  <Link
                    key={season.seasonNumber}
                    to="/library/tv/$tmdbId/season/$seasonNumber"
                    params={{ tmdbId, seasonNumber: String(season.seasonNumber) }}
                    className="group block max-w-xs"
                  >
                    <div className="rounded-lg bg-card border border-border p-4 hover:border-accent/50 transition-colors">
                      <div className="flex items-center justify-between mb-3">
                        <h3 className="font-medium text-foreground group-hover:text-accent transition-colors">
                          {season.name}
                        </h3>
                        <svg
                          aria-hidden="true"
                          className="w-4 h-4 text-muted group-hover:text-accent transition-colors"
                          fill="none"
                          stroke="currentColor"
                          viewBox="0 0 24 24"
                        >
                          <path
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            strokeWidth={2}
                            d="M9 5l7 7-7 7"
                          />
                        </svg>
                      </div>

                      <div className="space-y-2">
                        <div className="flex items-center justify-between text-sm">
                          <span className="text-muted">Episodes</span>
                          <span className="text-foreground">
                            {season.downloadedCount} / {season.episodeCount}
                          </span>
                        </div>

                        <div className="h-1.5 bg-background rounded-full overflow-hidden">
                          <div
                            className={`h-full transition-all duration-300 ${
                              seasonProgress === 100
                                ? 'bg-green-500'
                                : seasonProgress > 0
                                  ? 'bg-accent'
                                  : 'bg-muted/30'
                            }`}
                            style={{ width: `${seasonProgress}%` }}
                          />
                        </div>
                      </div>
                    </div>
                  </Link>
                )
              })}
          </div>
        )}
      </div>
    </div>
  )
}
