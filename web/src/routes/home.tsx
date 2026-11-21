import { useQuery } from '@tanstack/react-query'
import { HeroSection } from '../components/HeroSection'
import { movieLanes, tvLanes } from '../lib/api-client'
import { useAuth } from '../lib/auth'
import { ContentLane } from './requests/ContentLane'

export default function Home() {
  const { isAdmin } = useAuth()

  const { data: trendingMovies } = useQuery({
    queryKey: ['trending-movies'],
    queryFn: movieLanes.trending,
    staleTime: 1000 * 60 * 10,
  })

  const { data: trendingTv } = useQuery({
    queryKey: ['trending-tv'],
    queryFn: tvLanes.trending,
    staleTime: 1000 * 60 * 10,
  })

  const { data: upcoming } = useQuery({
    queryKey: ['upcoming-movies'],
    queryFn: movieLanes.upcoming,
    staleTime: 1000 * 60 * 60,
  })

  const { data: popularMovies } = useQuery({
    queryKey: ['popular-movies'],
    queryFn: movieLanes.popular,
    staleTime: 1000 * 60 * 60,
  })

  const { data: popularTv } = useQuery({
    queryKey: ['popular-tv'],
    queryFn: tvLanes.popular,
    staleTime: 1000 * 60 * 60,
  })

  const heroItems = trendingMovies?.results ?? []

  return (
    <div className="min-h-screen">
      {heroItems.length > 0 && <HeroSection items={heroItems} />}

      <div className="px-6 pt-8 -mt-16 relative z-10">
        <ContentLane title="Trending Movies" data={trendingMovies} isAdmin={isAdmin} />
        <ContentLane title="Trending TV Shows" data={trendingTv} isAdmin={isAdmin} />
        <ContentLane title="Upcoming Movies" data={upcoming} isAdmin={isAdmin} />
        <ContentLane title="Popular Movies" data={popularMovies} isAdmin={isAdmin} />
        <ContentLane title="Popular TV Shows" data={popularTv} isAdmin={isAdmin} />
      </div>
    </div>
  )
}
