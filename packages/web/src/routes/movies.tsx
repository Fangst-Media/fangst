import { useQuery } from '@tanstack/react-query'
import { useAuth } from '../lib/auth'
import { movieLanes } from '../lib/api-client'
import { HeroSection } from '../components/HeroSection'
import { ContentLane } from './requests/ContentLane'

export default function Movies() {
  const { isAdmin } = useAuth()

  const { data: trending } = useQuery({
    queryKey: ['trending-movies'],
    queryFn: movieLanes.trending,
    staleTime: 1000 * 60 * 10,
  })

  const { data: nowPlaying } = useQuery({
    queryKey: ['now-playing-movies'],
    queryFn: movieLanes.nowPlaying,
    staleTime: 1000 * 60 * 60,
  })

  const { data: upcoming } = useQuery({
    queryKey: ['upcoming-movies'],
    queryFn: movieLanes.upcoming,
    staleTime: 1000 * 60 * 60,
  })

  const { data: popular } = useQuery({
    queryKey: ['popular-movies'],
    queryFn: movieLanes.popular,
    staleTime: 1000 * 60 * 60,
  })

  const { data: topRated } = useQuery({
    queryKey: ['top-rated-movies'],
    queryFn: movieLanes.topRated,
    staleTime: 1000 * 60 * 60,
  })

  const heroItems = trending?.results ?? []

  return (
    <div className="min-h-screen">
      {heroItems.length > 0 && <HeroSection items={heroItems} />}

      <div className="px-6 pt-8 -mt-16 relative z-10">
        <ContentLane title="Trending" data={trending} isAdmin={isAdmin} />
        <ContentLane title="Now Playing" data={nowPlaying} isAdmin={isAdmin} />
        <ContentLane title="Upcoming" data={upcoming} isAdmin={isAdmin} />
        <ContentLane title="Popular" data={popular} isAdmin={isAdmin} />
        <ContentLane title="Top Rated" data={topRated} isAdmin={isAdmin} />
      </div>
    </div>
  )
}
