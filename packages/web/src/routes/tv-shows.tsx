import { useQuery } from '@tanstack/react-query'
import { useAuth } from '../lib/auth'
import { tvLanes } from '../lib/api-client'
import { HeroSection } from '../components/HeroSection'
import { ContentLane } from './requests/ContentLane'

export default function TvShows() {
  const { isAdmin } = useAuth()

  const { data: trending } = useQuery({
    queryKey: ['trending-tv'],
    queryFn: tvLanes.trending,
    staleTime: 1000 * 60 * 10,
  })

  const { data: onAir } = useQuery({
    queryKey: ['on-air-tv'],
    queryFn: tvLanes.onTheAir,
    staleTime: 1000 * 60 * 60,
  })

  const { data: popular } = useQuery({
    queryKey: ['popular-tv'],
    queryFn: tvLanes.popular,
    staleTime: 1000 * 60 * 60,
  })

  const { data: topRated } = useQuery({
    queryKey: ['top-rated-tv'],
    queryFn: tvLanes.topRated,
    staleTime: 1000 * 60 * 60,
  })

  const heroItems = trending?.results ?? []

  return (
    <div className="min-h-screen">
      {heroItems.length > 0 && <HeroSection items={heroItems} />}

      <div className="px-6 pt-8 -mt-16 relative z-10">
        <ContentLane title="Trending" data={trending} isAdmin={isAdmin} />
        <ContentLane title="On Air" data={onAir} isAdmin={isAdmin} />
        <ContentLane title="Popular" data={popular} isAdmin={isAdmin} />
        <ContentLane title="Top Rated" data={topRated} isAdmin={isAdmin} />
      </div>
    </div>
  )
}
