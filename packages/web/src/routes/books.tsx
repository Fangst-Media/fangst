import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useAuth } from '../lib/auth'
import { bookLanes } from '../lib/api-client'
import { HeroSection } from '../components/HeroSection'
import { ContentLane } from './requests/ContentLane'

type BookFormat = 'ebook' | 'audiobook'

export default function Books() {
  const { isAdmin } = useAuth()
  const [format, setFormat] = useState<BookFormat>('ebook')

  const { data: trending } = useQuery({
    queryKey: ['trending-books'],
    queryFn: bookLanes.trending,
    staleTime: 1000 * 60 * 60,
  })

  const { data: popular } = useQuery({
    queryKey: ['popular-books'],
    queryFn: bookLanes.popular,
    staleTime: 1000 * 60 * 60,
  })

  const { data: classics } = useQuery({
    queryKey: ['classic-books'],
    queryFn: bookLanes.classics,
    staleTime: 1000 * 60 * 60,
  })

  const heroItems = trending?.results ?? []

  return (
    <div className="min-h-screen">
      {heroItems.length > 0 && <HeroSection items={heroItems} />}

      <div className="px-6 pt-8 -mt-16 relative z-10">
        {/* Format toggle */}
        <div className="flex items-center gap-1 mb-6 bg-card rounded-lg p-1 w-fit border border-border">
          <button
            type="button"
            onClick={() => setFormat('ebook')}
            className={`px-4 py-1.5 rounded-md text-sm font-medium transition-colors ${
              format === 'ebook' ? 'bg-accent text-white' : 'text-muted hover:text-foreground'
            }`}
          >
            Ebooks
          </button>
          <button
            type="button"
            onClick={() => setFormat('audiobook')}
            className={`px-4 py-1.5 rounded-md text-sm font-medium transition-colors ${
              format === 'audiobook' ? 'bg-accent text-white' : 'text-muted hover:text-foreground'
            }`}
          >
            Audiobooks
          </button>
        </div>

        <ContentLane title="Trending" data={trending} isAdmin={isAdmin} />
        <ContentLane title="Popular" data={popular} isAdmin={isAdmin} />
        <ContentLane title="Classics" data={classics} isAdmin={isAdmin} />
      </div>
    </div>
  )
}
