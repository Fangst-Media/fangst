import { useNavigate } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import type { TrendingItem } from '../routes/requests/TrendingLane'

type HeroSectionProps = {
  items: TrendingItem[]
  interval?: number
}

export function HeroSection({ items, interval = 8000 }: HeroSectionProps) {
  const navigate = useNavigate()
  const [activeIndex, setActiveIndex] = useState(0)
  const [transitioning, setTransitioning] = useState(false)

  const heroItems = items.filter((i) => i.backdrop || i.poster).slice(0, 5)

  function goTo(index: number) {
    setTransitioning(true)
    setTimeout(() => {
      setActiveIndex(index)
      setTransitioning(false)
    }, 300)
  }

  useEffect(() => {
    if (heroItems.length <= 1) return
    const timer = setInterval(() => {
      setTransitioning(true)
      setTimeout(() => {
        setActiveIndex((prev) => (prev + 1) % heroItems.length)
        setTransitioning(false)
      }, 300)
    }, interval)
    return () => clearInterval(timer)
  }, [heroItems.length, interval])

  if (heroItems.length === 0) return null

  const item = heroItems[activeIndex]!
  const hasBackdrop = !!item.backdrop
  const imageUrl = item.backdrop
    ? item.backdrop.startsWith('http')
      ? item.backdrop
      : `https://image.tmdb.org/t/p/original${item.backdrop}`
    : item.poster?.startsWith('http')
      ? item.poster
      : `https://image.tmdb.org/t/p/original${item.poster}`
  const detailPath =
    item.type === 'book'
      ? `/book/${item.id}`
      : item.type === 'movie'
        ? `/movie/${item.id}`
        : `/tv/${item.id}`

  return (
    <div className="-mt-16 relative h-[70vh] min-h-[500px] w-full overflow-hidden">
      <img
        key={item.id}
        src={imageUrl}
        alt=""
        className={`absolute inset-0 w-full h-full object-cover transition-opacity duration-500 ${
          !hasBackdrop ? 'scale-110 blur-2xl opacity-50' : ''
        } ${transitioning ? 'opacity-0' : ''}`}
      />

      {/* Gradient overlays */}
      <div className="absolute inset-0 bg-gradient-to-t from-background via-background/40 to-transparent" />
      <div className="absolute inset-0 bg-gradient-to-r from-background/80 via-transparent to-transparent" />

      {/* Content */}
      <div className="absolute bottom-0 left-0 right-0 px-6 pb-20 max-w-[1920px] mx-auto">
        <div className="max-w-xl">
          <h1
            key={`title-${item.id}`}
            className={`text-4xl md:text-5xl font-bold text-foreground mb-3 drop-shadow-lg transition-opacity duration-300 ${
              transitioning ? 'opacity-0' : 'opacity-100'
            }`}
          >
            {item.title}
          </h1>
          <div
            className={`flex items-center gap-3 mb-4 text-sm text-foreground/70 transition-opacity duration-300 ${
              transitioning ? 'opacity-0' : 'opacity-100'
            }`}
          >
            {item.year && <span>{item.year}</span>}
            {item.rating !== undefined && item.rating > 0 && (
              <span className="flex items-center gap-1">
                <svg
                  aria-hidden="true"
                  className="w-4 h-4 text-yellow-400"
                  fill="currentColor"
                  viewBox="0 0 20 20"
                >
                  <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                </svg>
                {item.rating.toFixed(1)}
              </span>
            )}
            <span className="px-2 py-0.5 rounded bg-foreground/10 text-xs uppercase">
              {item.type}
            </span>
          </div>
          {item.overview && (
            <p
              className={`text-foreground/80 text-sm md:text-base line-clamp-3 mb-6 leading-relaxed transition-opacity duration-300 ${
                transitioning ? 'opacity-0' : 'opacity-100'
              }`}
            >
              {item.overview}
            </p>
          )}
          <div
            className={`flex items-center gap-3 transition-opacity duration-300 ${
              transitioning ? 'opacity-0' : 'opacity-100'
            }`}
          >
            <button
              type="button"
              onClick={() => navigate({ to: detailPath })}
              className="flex items-center gap-2 rounded-md bg-foreground px-6 py-2.5 text-sm font-semibold text-background hover:bg-foreground/90 transition-colors"
            >
              <svg
                aria-hidden="true"
                className="w-5 h-5"
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
              More Info
            </button>
          </div>
        </div>

        {/* Slide indicators */}
        {heroItems.length > 1 && (
          <div className="absolute bottom-6 right-6 flex items-center gap-2">
            {heroItems.map((item, i) => (
              <button
                key={item.id}
                type="button"
                onClick={() => goTo(i)}
                className={`h-1 rounded-full transition-all duration-300 ${
                  i === activeIndex
                    ? 'w-6 bg-foreground'
                    : 'w-2 bg-foreground/40 hover:bg-foreground/60'
                }`}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
