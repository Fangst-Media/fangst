import { useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { SimpleRequestModal } from '../components/SimpleRequestModal'
import { useAuth } from '../lib/auth'
import { ReleaseModal, type SearchResult } from './requests/ReleaseModal'
import { TrendingLane, type TrendingItem } from './requests/TrendingLane'

async function fetchTrendingMovies(): Promise<TrendingItem[]> {
  const response = await fetch('/api/metadata/tmdb/movies/trending', { credentials: 'include' })
  if (!response.ok) return []
  const data = await response.json()
  return data.results || []
}

async function fetchTrendingTv(): Promise<TrendingItem[]> {
  const response = await fetch('/api/metadata/tmdb/tv/trending', { credentials: 'include' })
  if (!response.ok) return []
  const data = await response.json()
  return data.results || []
}

async function searchContent(query: string): Promise<SearchResult[]> {
  if (!query.trim()) return []

  const [movies, tv] = await Promise.all([
    fetch(`/api/metadata/tmdb/movies/search?query=${encodeURIComponent(query)}`, {
      credentials: 'include',
    }).then((r) => r.json()),
    fetch(`/api/metadata/tmdb/tv/search?query=${encodeURIComponent(query)}`, {
      credentials: 'include',
    }).then((r) => r.json()),
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
    setSelectedMedia({
      id: item.id,
      title: item.title,
      year: item.year,
      overview: item.overview,
      posterPath: item.poster,
      type: item.type,
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
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
              />
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
              />
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
      {selectedMedia &&
        (requestMode === 'simple' ? (
          <SimpleRequestModal media={selectedMedia} onClose={() => setSelectedMedia(null)} />
        ) : (
          <ReleaseModal media={selectedMedia} onClose={() => setSelectedMedia(null)} />
        ))}
    </div>
  )
}
