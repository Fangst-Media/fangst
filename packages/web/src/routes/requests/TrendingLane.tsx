import type { MetadataResult } from '@jaeger/shared/schemas/metadata'
import { useRef } from 'react'
import { useNavigate } from '@tanstack/react-router'

export type TrendingItem = MetadataResult

function isNotReleased(releaseDate?: string): boolean {
  if (!releaseDate) return false
  const today = new Date().toISOString().split('T')[0]!
  return releaseDate > today
}

function formatReleaseDate(releaseDate: string): string {
  const date = new Date(releaseDate + 'T00:00:00')
  return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
}

export function TrendingLane({
  title,
  items,
  onSelect,
}: {
  title: string
  items: TrendingItem[]
  onSelect?: (item: TrendingItem) => void
}) {
  const scrollRef = useRef<HTMLDivElement>(null)
  const navigate = useNavigate()

  const scroll = (direction: 'left' | 'right') => {
    if (scrollRef.current) {
      const scrollAmount = scrollRef.current.clientWidth * 0.75
      scrollRef.current.scrollBy({
        left: direction === 'left' ? -scrollAmount : scrollAmount,
        behavior: 'smooth',
      })
    }
  }

  function handleSelect(item: TrendingItem) {
    if (onSelect) {
      onSelect(item)
      return
    }
    const path = item.type === 'book' ? `/book/${item.id}` : item.type === 'movie' ? `/movie/${item.id}` : `/tv/${item.id}`
    navigate({ to: path })
  }

  if (items.length === 0) return null

  return (
    <div className="mb-8">
      <h2 className="text-lg font-semibold text-foreground mb-3">{title}</h2>
      <div className="relative group">
        <button
          type="button"
          onClick={() => scroll('left')}
          className="absolute left-0 top-1/2 -translate-y-1/2 z-10 w-10 h-10 rounded-full bg-background/80 border border-border flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity hover:bg-card"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M15 19l-7-7 7-7"
            />
          </svg>
        </button>
        <div
          ref={scrollRef}
          className="flex gap-2 overflow-x-auto scrollbar-hide pb-2"
          style={{ scrollbarWidth: 'none', msOverflowStyle: 'none' }}
        >
          {items.slice(0, 20).map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => handleSelect(item)}
              className="shrink-0 w-[168px] group/card text-left"
            >
              <div className="relative aspect-[2/3] rounded-lg overflow-hidden bg-card border border-border group-hover/card:border-accent/50 transition-colors">
                {item.poster ? (
                  <img
                    src={item.poster.startsWith('http') ? item.poster : `https://image.tmdb.org/t/p/w342${item.poster}`}
                    alt={item.title}
                    className="w-full h-full object-cover"
                    loading="lazy"
                  />
                ) : (
                  <div className="w-full h-full flex items-center justify-center bg-card">
                    <svg
                      className="w-8 h-8 text-muted"
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
                  </div>
                )}
                {item.type === 'movie' && isNotReleased(item.releaseDate) && (
                  <div className="absolute top-1 right-1 px-1.5 py-0.5 text-[10px] font-medium rounded bg-orange-500 text-white shadow">
                    {formatReleaseDate(item.releaseDate!)}
                  </div>
                )}
                <div className="absolute bottom-0 left-0 right-0 p-2 bg-gradient-to-t from-black/80 to-transparent opacity-0 group-hover/card:opacity-100 transition-opacity">
                  <span className="text-xs text-white font-medium">View Details</span>
                </div>
              </div>
              <p className="mt-2 text-sm text-foreground truncate">{item.title}</p>
              {item.author && <p className="text-xs text-muted truncate">{item.author}</p>}
              {item.year && !item.author && <p className="text-xs text-muted">{item.year}</p>}
            </button>
          ))}
        </div>
        <button
          type="button"
          onClick={() => scroll('right')}
          className="absolute right-0 top-1/2 -translate-y-1/2 z-10 w-10 h-10 rounded-full bg-background/80 border border-border flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity hover:bg-card"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </div>
    </div>
  )
}
