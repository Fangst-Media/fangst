import { useState } from 'react'
import { Link } from '@tanstack/react-router'

type CollectionCardProps = {
  tmdbCollectionId: number
  name: string
  posterPath?: string
  movieCount: number
  downloadedCount: number
}

export function CollectionCard({
  tmdbCollectionId,
  name,
  posterPath,
  movieCount,
  downloadedCount,
}: CollectionCardProps) {
  const [imgError, setImgError] = useState(false)
  const posterUrl = posterPath
    ? `https://image.tmdb.org/t/p/w185${posterPath}`
    : '/placeholder-poster.png'

  const isComplete = downloadedCount >= movieCount

  return (
    <Link
      to="/library/collection/$collectionId"
      params={{ collectionId: tmdbCollectionId.toString() }}
      className="group cursor-pointer block"
    >
      <div className="relative aspect-[2/3] overflow-hidden rounded-md bg-card mb-2">
        {!imgError ? (
          <img
            src={posterUrl}
            alt={name}
            className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-105"
            loading="lazy"
            onError={() => setImgError(true)}
          />
        ) : (
          <div className="h-full w-full flex items-center justify-center bg-card">
            <div className="text-center p-4">
              <svg
                className="w-12 h-12 text-muted mx-auto mb-2"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"
                />
              </svg>
              <p className="text-xs text-muted line-clamp-2">{name}</p>
            </div>
          </div>
        )}

        {/* Hover overlay */}
        <div className="absolute inset-0 bg-black/40 opacity-0 transition-opacity duration-200 group-hover:opacity-100 flex items-center justify-center">
          <div className="rounded-full bg-white/20 p-3">
            <svg className="w-6 h-6 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"
              />
            </svg>
          </div>
        </div>

        {/* Collection badge */}
        <div className="absolute top-2 left-2">
          <span className="rounded bg-purple-500/90 px-1.5 py-0.5 text-[10px] font-medium text-white uppercase">
            Collection
          </span>
        </div>

        {/* Progress indicator */}
        <div className="absolute bottom-2 right-2">
          <span
            className={`rounded px-1.5 py-0.5 text-[10px] font-medium text-white ${
              isComplete ? 'bg-green-500/90' : 'bg-accent'
            }`}
          >
            {downloadedCount}/{movieCount}
          </span>
        </div>
      </div>

      <div className="px-0.5">
        <h3 className="text-sm font-medium text-foreground line-clamp-1 group-hover:text-accent transition-colors">
          {name}
        </h3>
        <p className="text-xs text-muted mt-0.5">
          {isComplete ? 'Complete' : `${movieCount - downloadedCount} missing`}
        </p>
      </div>
    </Link>
  )
}
