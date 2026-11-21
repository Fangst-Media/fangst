import { Link } from '@tanstack/react-router'
import { useState } from 'react'

type TvShowCardProps = {
  tmdbId: string
  title: string
  year?: number
  posterPath?: string
  rating?: number
  episodeCount: number
}

function getPosterUrl(path?: string): string {
  if (!path) return '/placeholder-poster.png'
  return `https://image.tmdb.org/t/p/w342${path}`
}

export function TvShowCard({
  tmdbId,
  title,
  year,
  posterPath,
  rating,
  episodeCount,
}: TvShowCardProps) {
  const [imgError, setImgError] = useState(false)
  const posterUrl = getPosterUrl(posterPath)

  return (
    <Link to="/library/tv/$tmdbId" params={{ tmdbId }} className="group cursor-pointer block">
      <div className="relative aspect-[2/3] overflow-hidden rounded-md bg-card mb-2">
        {!imgError ? (
          <img
            src={posterUrl}
            alt={title}
            className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-105"
            loading="lazy"
            onError={() => setImgError(true)}
          />
        ) : (
          <div className="h-full w-full flex items-center justify-center bg-card">
            <div className="text-center p-4">
              <svg
                aria-hidden="true"
                className="w-12 h-12 text-muted mx-auto mb-2"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z"
                />
              </svg>
              <p className="text-xs text-muted line-clamp-2">{title}</p>
            </div>
          </div>
        )}

        {/* Hover overlay with info icon */}
        <div className="absolute inset-0 bg-black/40 opacity-0 transition-opacity duration-200 group-hover:opacity-100 flex items-center justify-center">
          <div className="rounded-full bg-white/20 p-3">
            <svg
              aria-hidden="true"
              className="w-6 h-6 text-white"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
          </div>
        </div>

        {rating && rating > 0 && (
          <div className="absolute top-2 right-2 flex items-center gap-1 rounded bg-black/70 px-1.5 py-0.5">
            <span className="text-yellow-400 text-xs">★</span>
            <span className="text-[10px] font-medium text-white">{rating.toFixed(1)}</span>
          </div>
        )}

        <div className="absolute bottom-2 right-2">
          <span className="rounded bg-accent px-1.5 py-0.5 text-[10px] font-medium text-white">
            {episodeCount} {episodeCount === 1 ? 'episode' : 'episodes'}
          </span>
        </div>
      </div>

      <div className="px-0.5">
        <h3 className="text-sm font-medium text-foreground line-clamp-1 group-hover:text-accent transition-colors">
          {title}
        </h3>
        {year && <p className="text-xs text-muted mt-0.5">{year}</p>}
      </div>
    </Link>
  )
}
