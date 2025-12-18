import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { getMedia, triggerScan, type MediaItem } from '../lib/api'

function countUniqueShows(items: MediaItem[]): number {
  const shows = new Set<string>()
  for (const item of items) {
    const key = item.metadata?.id || item.parsed.title.toLowerCase()
    shows.add(key)
  }
  return shows.size
}

export default function Dashboard() {
  const queryClient = useQueryClient()
  const [isScanning, setIsScanning] = useState(false)

  const { data } = useQuery({
    queryKey: ['media'],
    queryFn: getMedia,
  })

  async function handleScan() {
    setIsScanning(true)
    try {
      await triggerScan()
      await queryClient.invalidateQueries({ queryKey: ['media'] })
    } finally {
      setIsScanning(false)
    }
  }

  const movies = data?.results.filter((item) => !item.parsed.season) || []
  const tvEpisodes = data?.results.filter((item) => item.parsed.season) || []
  const tvShowCount = countUniqueShows(tvEpisodes)

  return (
    <div className="min-h-screen p-6">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-foreground">Dashboard</h1>
        <p className="text-sm text-muted">Overview of your media library</p>
      </div>

      <div>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
          <StatCard label="Movies" value={movies.length} />
          <StatCard label="TV Shows" value={tvShowCount} />
          <StatCard label="Total Items" value={movies.length + tvShowCount} />
          <StatCard label="Pending Requests" value={0} />
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <section className="rounded-lg bg-card border border-border p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Quick Actions</h2>
            <div className="space-y-2">
              <button
                type="button"
                onClick={handleScan}
                disabled={isScanning}
                className="w-full flex items-center gap-3 p-3 rounded-md hover:bg-background transition-colors text-left disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <div className="w-10 h-10 rounded-md bg-accent/10 flex items-center justify-center">
                  {isScanning ? (
                    <div className="h-5 w-5 animate-spin rounded-full border-2 border-accent border-t-transparent" />
                  ) : (
                    <svg className="w-5 h-5 text-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                    </svg>
                  )}
                </div>
                <div>
                  <p className="font-medium text-foreground">{isScanning ? 'Scanning...' : 'Scan Library'}</p>
                  <p className="text-sm text-muted">Scan filesystem for new media</p>
                </div>
              </button>

              <Link
                to="/library"
                className="flex items-center gap-3 p-3 rounded-md hover:bg-background transition-colors"
              >
                <div className="w-10 h-10 rounded-md bg-accent/10 flex items-center justify-center">
                  <svg className="w-5 h-5 text-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z" />
                  </svg>
                </div>
                <div>
                  <p className="font-medium text-foreground">Browse Library</p>
                  <p className="text-sm text-muted">View all media in your collection</p>
                </div>
              </Link>

              <Link
                to="/requests"
                className="flex items-center gap-3 p-3 rounded-md hover:bg-background transition-colors"
              >
                <div className="w-10 h-10 rounded-md bg-accent/10 flex items-center justify-center">
                  <svg className="w-5 h-5 text-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                  </svg>
                </div>
                <div>
                  <p className="font-medium text-foreground">Requests</p>
                  <p className="text-sm text-muted">View and manage content requests</p>
                </div>
              </Link>
            </div>
          </section>

          <section className="rounded-lg bg-card border border-border p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Recent Activity</h2>
            <div className="flex items-center justify-center h-32 text-muted text-sm">
              No recent activity
            </div>
          </section>
        </div>
      </div>
    </div>
  )
}

function StatCard({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-lg bg-card border border-border p-4">
      <p className="text-sm text-muted">{label}</p>
      <p className="text-3xl font-bold text-foreground mt-1">{value}</p>
    </div>
  )
}
