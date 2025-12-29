import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { getMedia, triggerScan, type MediaItem } from '../lib/api'
import { MediaCard } from '../components/MediaCard'
import { TvShowCard } from '../components/TvShowCard'
import { CollectionCard } from '../components/CollectionCard'
import { useAuth } from '../lib/auth'

type CollectionOverview = {
  id: string
  tmdbCollectionId: number
  name: string
  posterPath?: string
  backdropPath?: string
  movieCount: number
  downloadedCount: number
}

async function getCollections(): Promise<{ collections: CollectionOverview[] }> {
  const response = await fetch('/api/library/collections', { credentials: 'include' })
  if (!response.ok) throw new Error('Failed to fetch collections')
  return response.json()
}

type GroupedShow = {
  key: string
  title: string
  year?: number
  metadata?: MediaItem['metadata']
  episodeCount: number
  episodes: MediaItem[]
}

function groupTvShows(items: MediaItem[]): GroupedShow[] {
  const groups = new Map<string, GroupedShow>()

  for (const item of items) {
    const key = item.metadata?.id || item.parsed.title.toLowerCase()
    const existing = groups.get(key)

    if (existing) {
      existing.episodes.push(item)
      existing.episodeCount++
    } else {
      groups.set(key, {
        key,
        title: item.metadata?.title || item.parsed.title,
        year: item.metadata?.year || item.parsed.year,
        metadata: item.metadata,
        episodeCount: 1,
        episodes: [item],
      })
    }
  }

  return Array.from(groups.values())
}

export default function Library() {
  const { isAdmin } = useAuth()
  const queryClient = useQueryClient()
  const [isScanning, setIsScanning] = useState(false)

  const { data, isLoading, error } = useQuery({
    queryKey: ['media'],
    queryFn: getMedia,
  })

  const { data: collectionsData } = useQuery({
    queryKey: ['collections'],
    queryFn: getCollections,
  })

  async function handleScan() {
    setIsScanning(true)
    try {
      await triggerScan()
      await queryClient.invalidateQueries({ queryKey: ['media'] })
    } finally {
      setIsScanning(false)
    }
  }

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="text-center">
          <div className="mb-4 h-10 w-10 animate-spin rounded-full border-2 border-accent border-t-transparent mx-auto" />
          <p className="text-sm text-muted">Loading library...</p>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex min-h-screen items-center justify-center p-4">
        <div className="rounded-lg bg-card border border-border p-6 max-w-md text-center">
          <div className="w-12 h-12 rounded-full bg-red-500/10 flex items-center justify-center mx-auto mb-4">
            <svg className="w-6 h-6 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          </div>
          <h2 className="text-lg font-semibold text-foreground mb-2">Failed to load library</h2>
          <p className="text-sm text-muted">{error.message}</p>
        </div>
      </div>
    )
  }

  const movies = data?.results.filter((item) => !item.parsed.season) || []
  const tvEpisodes = data?.results.filter((item) => item.parsed.season) || []
  const groupedShows = groupTvShows(tvEpisodes)
  const collections = collectionsData?.collections || []
  const totalItems = movies.length + groupedShows.length + collections.length

  return (
    <div className="min-h-screen p-6">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Library</h1>
          <p className="text-sm text-muted">Browse your media collection</p>
        </div>
        {isAdmin && (
          <button
            type="button"
            onClick={handleScan}
            disabled={isScanning}
            className="flex items-center gap-2 rounded-md bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent/90 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isScanning ? (
              <>
                <div className="h-4 w-4 animate-spin rounded-full border-2 border-white border-t-transparent" />
                Scanning...
              </>
            ) : (
              <>
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                </svg>
                Scan Library
              </>
            )}
          </button>
        )}
      </div>

      <div>
        {/* Collections section */}
        {collections.length > 0 && (
          <section className="mb-10">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold text-foreground">Collections</h2>
              <span className="text-sm text-muted">{collections.length}</span>
            </div>
            <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 2xl:grid-cols-8 gap-4">
              {collections.map((collection) => (
                <CollectionCard
                  key={collection.id}
                  tmdbCollectionId={collection.tmdbCollectionId}
                  name={collection.name}
                  posterPath={collection.posterPath}
                  movieCount={collection.movieCount}
                  downloadedCount={collection.downloadedCount}
                />
              ))}
            </div>
          </section>
        )}

        {movies.length > 0 && (
          <section className="mb-10">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold text-foreground">Movies</h2>
              <span className="text-sm text-muted">{movies.length}</span>
            </div>
            <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 2xl:grid-cols-8 gap-4">
              {movies.map((item) => (
                <MediaCard key={item.path} item={item} />
              ))}
            </div>
          </section>
        )}

        {groupedShows.length > 0 && (
          <section>
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold text-foreground">TV Shows</h2>
              <span className="text-sm text-muted">{groupedShows.length}</span>
            </div>
            <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 2xl:grid-cols-8 gap-4">
              {groupedShows.map((show) => {
                const tmdbId = show.metadata?.id?.replace(/^[^:]+:/, '')
                if (tmdbId) {
                  return (
                    <TvShowCard
                      key={show.key}
                      tmdbId={tmdbId}
                      title={show.title}
                      year={show.year}
                      posterPath={show.metadata?.posterPath}
                      rating={show.metadata?.rating}
                      episodeCount={show.episodeCount}
                    />
                  )
                }
                return (
                  <MediaCard key={show.key} item={show.episodes[0]!} episodeCount={show.episodeCount} />
                )
              })}
            </div>
          </section>
        )}

        {totalItems === 0 && (
          <div className="flex flex-col items-center justify-center py-24">
            <div className="w-16 h-16 rounded-full bg-card flex items-center justify-center mb-4">
              <svg className="w-8 h-8 text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z" />
              </svg>
            </div>
            <h3 className="text-lg font-medium text-foreground mb-1">No media found</h3>
            <p className="text-sm text-muted text-center max-w-sm">
              Add media files to your library folder to see them here
            </p>
          </div>
        )}
      </div>
    </div>
  )
}
