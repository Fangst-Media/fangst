import { useState, useEffect } from 'react'
import { useNavigate } from '@tanstack/react-router'
import type { TrendingItem } from '../routes/requests/TrendingLane'

type BookHeroSectionProps = {
  items: TrendingItem[]
  interval?: number
}

export function BookHeroSection({ items, interval = 8000 }: BookHeroSectionProps) {
  const navigate = useNavigate()
  const [activeIndex, setActiveIndex] = useState(0)
  const [transitioning, setTransitioning] = useState(false)

  const heroItems = items.filter((i) => i.poster).slice(0, 5)

  function goTo(index: number) {
    if (index === activeIndex) return
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

  const featured = heroItems[activeIndex]!
  const coverUrl = (poster: string) =>
    poster.startsWith('http') ? poster : `https://image.tmdb.org/t/p/w500${poster}`

  return (
    <div className="-mt-16 relative w-full overflow-hidden bg-gradient-to-b from-card via-background/80 to-background">
      {/* Blurred background from active cover */}
      <div className="absolute inset-0 overflow-hidden">
        <img
          key={featured.id}
          src={coverUrl(featured.poster!)}
          alt=""
          className={`absolute inset-0 w-full h-full object-cover scale-150 blur-3xl opacity-15 transition-opacity duration-500 ${
            transitioning ? 'opacity-0' : ''
          }`}
        />
        <div className="absolute inset-0 bg-gradient-to-b from-background/60 via-background/40 to-background" />
      </div>

      {/* Content */}
      <div className="relative px-6 pt-24 pb-16 max-w-[1920px] mx-auto">
        <div className="flex gap-10 items-start">
          {/* Featured cover */}
          <button
            type="button"
            onClick={() => navigate({ to: `/book/${featured.id}` })}
            className="shrink-0 group"
          >
            <div
              className={`w-[200px] md:w-[240px] aspect-[2/3] rounded-lg overflow-hidden shadow-2xl shadow-black/50 ring-1 ring-white/10 transition-all duration-300 group-hover:ring-accent/50 group-hover:shadow-accent/20 ${
                transitioning ? 'opacity-0 scale-95' : 'opacity-100 scale-100'
              }`}
            >
              <img
                key={`cover-${featured.id}`}
                src={coverUrl(featured.poster!)}
                alt={featured.title}
                className="w-full h-full object-cover"
              />
            </div>
          </button>

          {/* Info */}
          <div className={`flex-1 min-w-0 pt-4 transition-opacity duration-300 ${
            transitioning ? 'opacity-0' : 'opacity-100'
          }`}>
            <h1
              key={`title-${featured.id}`}
              className="text-3xl md:text-4xl font-bold text-foreground mb-2 drop-shadow-lg"
            >
              {featured.title}
            </h1>
            <div className="flex items-center gap-3 mb-4 text-sm text-foreground/60">
              {featured.author && <span>{featured.author}</span>}
              {featured.year && <span>{featured.year}</span>}
              {featured.rating !== undefined && featured.rating > 0 && (
                <span className="flex items-center gap-1">
                  <svg className="w-4 h-4 text-yellow-400" fill="currentColor" viewBox="0 0 20 20">
                    <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                  </svg>
                  {featured.rating.toFixed(1)}
                </span>
              )}
            </div>
            {featured.overview && (
              <p className="text-foreground/70 text-sm md:text-base line-clamp-4 mb-6 leading-relaxed max-w-lg">
                {featured.overview}
              </p>
            )}
            <button
              type="button"
              onClick={() => navigate({ to: `/book/${featured.id}` })}
              className="flex items-center gap-2 rounded-md bg-foreground px-5 py-2 text-sm font-semibold text-background hover:bg-foreground/90 transition-colors"
            >
              View Details
            </button>
          </div>
        </div>

        {/* Slide indicators */}
        {heroItems.length > 1 && (
          <div className="flex items-center gap-2 mt-8">
            {heroItems.map((_, i) => (
              <button
                key={i}
                type="button"
                onClick={() => goTo(i)}
                className={`h-1 rounded-full transition-all duration-300 ${
                  i === activeIndex ? 'w-6 bg-foreground' : 'w-2 bg-foreground/40 hover:bg-foreground/60'
                }`}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
