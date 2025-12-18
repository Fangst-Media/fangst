import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'

type SearchResult = {
  id: string
  title: string
  year?: number
  overview?: string
  posterPath?: string
  type: 'movie' | 'tv'
}

async function searchContent(query: string): Promise<SearchResult[]> {
  if (!query.trim()) return []

  const [movies, tv] = await Promise.all([
    fetch(`https://metadata-relay.dorninger.co/tmdb/movies/search?query=${encodeURIComponent(query)}`).then((r) =>
      r.json()
    ),
    fetch(`https://metadata-relay.dorninger.co/tmdb/tv/search?query=${encodeURIComponent(query)}`).then((r) =>
      r.json()
    ),
  ])

  const movieResults: SearchResult[] = movies.results?.slice(0, 5).map((m: { id: number; title: string; release_date?: string; overview?: string; poster_path?: string }) => ({
    id: m.id.toString(),
    title: m.title,
    year: m.release_date ? new Date(m.release_date).getFullYear() : undefined,
    overview: m.overview,
    posterPath: m.poster_path,
    type: 'movie' as const,
  })) || []

  const tvResults: SearchResult[] = tv.results?.slice(0, 5).map((t: { id: number; name: string; first_air_date?: string; overview?: string; poster_path?: string }) => ({
    id: t.id.toString(),
    title: t.name,
    year: t.first_air_date ? new Date(t.first_air_date).getFullYear() : undefined,
    overview: t.overview,
    posterPath: t.poster_path,
    type: 'tv' as const,
  })) || []

  return [...movieResults, ...tvResults]
}

export default function Requests() {
  const [query, setQuery] = useState('')
  const [debouncedQuery, setDebouncedQuery] = useState('')

  const { data: results, isLoading } = useQuery({
    queryKey: ['search', debouncedQuery],
    queryFn: () => searchContent(debouncedQuery),
    enabled: debouncedQuery.length > 2,
  })

  function handleSearch(value: string) {
    setQuery(value)
    const timeout = setTimeout(() => setDebouncedQuery(value), 300)
    return () => clearTimeout(timeout)
  }

  function getPosterUrl(path?: string) {
    if (!path) return '/placeholder-poster.png'
    return `https://image.tmdb.org/t/p/w185${path}`
  }

  return (
    <div className="min-h-screen p-6">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-foreground">Discover</h1>
        <p className="text-sm text-muted">Search and request new content</p>
      </div>

      <div className="max-w-4xl">
        <div className="mb-8">
          <label htmlFor="search" className="sr-only">
            Search for movies or TV shows
          </label>
          <div className="relative">
            <input
              id="search"
              type="text"
              value={query}
              onChange={(e) => handleSearch(e.target.value)}
              placeholder="Search for movies or TV shows..."
              className="w-full rounded-lg bg-card border border-border px-4 py-3 pl-12 text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent"
            />
            <svg
              className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-muted"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
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
          <div className="space-y-4">
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
                        {result.year && (
                          <span className="text-muted ml-2">({result.year})</span>
                        )}
                      </h3>
                      <span className="inline-block mt-1 px-2 py-0.5 text-xs rounded bg-accent/10 text-accent uppercase">
                        {result.type}
                      </span>
                    </div>
                    <button
                      type="button"
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
          <div className="text-center py-12">
            <div className="w-16 h-16 rounded-full bg-card flex items-center justify-center mx-auto mb-4">
              <svg className="w-8 h-8 text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                />
              </svg>
            </div>
            <h3 className="text-lg font-medium text-foreground mb-1">Search for content</h3>
            <p className="text-sm text-muted">Find movies and TV shows to request</p>
          </div>
        )}
      </div>
    </div>
  )
}
