import type { MetadataResult } from '@jaeger/shared/schemas/metadata'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { bookLanes, movieLanes, tvLanes } from '../lib/api-client'

async function searchContent(query: string): Promise<MetadataResult[]> {
  if (!query.trim()) return []

  const [movies, tv, books] = await Promise.all([
    movieLanes.search(query),
    tvLanes.search(query),
    bookLanes.search(query).catch(() => ({ results: [] })),
  ])

  return [...movies.results.slice(0, 10), ...tv.results.slice(0, 10), ...books.results.slice(0, 10)]
}

function getPosterUrl(result: MetadataResult): string {
  if (!result.poster) return '/placeholder-poster.png'
  if (result.poster.startsWith('http')) return result.poster
  return `https://image.tmdb.org/t/p/w185${result.poster}`
}

export default function Search() {
  const navigate = useNavigate()
  const [query, setQuery] = useState('')
  const [debouncedQuery, setDebouncedQuery] = useState('')

  useEffect(() => {
    const timeout = setTimeout(() => setDebouncedQuery(query), 400)
    return () => clearTimeout(timeout)
  }, [query])

  const { data: results, isLoading } = useQuery({
    queryKey: ['search', debouncedQuery],
    queryFn: () => searchContent(debouncedQuery),
    enabled: debouncedQuery.length > 2,
  })

  function handleSelect(result: MetadataResult) {
    const path =
      result.type === 'book'
        ? `/book/${result.id}`
        : result.type === 'movie'
          ? `/movie/${result.id}`
          : `/tv/${result.id}`
    navigate({ to: path })
  }

  return (
    <div className="min-h-screen px-6 py-8">
      <div className="max-w-3xl mx-auto">
        <div className="relative mb-8">
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search for movies, TV shows, or books..."
            className="w-full rounded-lg bg-card border border-border px-4 py-3 pl-12 text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent"
            // biome-ignore lint/a11y/noAutofocus: the page exists to type a query, so focus goes straight to it
            autoFocus
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

        {isLoading && (
          <div className="flex justify-center py-12">
            <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
          </div>
        )}

        {results && results.length > 0 && (
          <div className="space-y-3">
            {results.map((result) => (
              <button
                key={`${result.type}-${result.id}`}
                type="button"
                onClick={() => handleSelect(result)}
                className="flex gap-4 p-4 rounded-lg bg-card border border-border hover:border-accent/50 transition-colors w-full text-left"
              >
                <img
                  src={getPosterUrl(result)}
                  alt={result.title}
                  className="w-16 h-24 rounded object-cover bg-background shrink-0"
                />
                <div className="flex-1 min-w-0">
                  <h3 className="font-medium text-foreground">
                    {result.title}
                    {result.year && <span className="text-muted ml-2">({result.year})</span>}
                  </h3>
                  {result.author && <p className="text-sm text-muted">{result.author}</p>}
                  <span className="inline-block mt-1 px-2 py-0.5 text-xs rounded bg-accent/10 text-accent uppercase">
                    {result.type}
                  </span>
                  {result.overview && (
                    <p className="mt-2 text-sm text-muted line-clamp-2">{result.overview}</p>
                  )}
                </div>
              </button>
            ))}
          </div>
        )}

        {debouncedQuery.length > 2 && !isLoading && results?.length === 0 && (
          <div className="text-center py-12">
            <p className="text-muted">No results found for "{debouncedQuery}"</p>
          </div>
        )}

        {!debouncedQuery && (
          <div className="text-center py-24">
            <svg
              aria-hidden="true"
              className="w-16 h-16 text-muted/30 mx-auto mb-4"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={1}
                d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
              />
            </svg>
            <p className="text-muted">Start typing to search</p>
          </div>
        )}
      </div>
    </div>
  )
}
