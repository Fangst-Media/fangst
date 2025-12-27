import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { getPosterUrl, parseMediaSlug, type MediaItem } from '../lib/api'

type MediaDetailsProps = {
  slug: string
}

async function fetchMediaItem(slug: string): Promise<MediaItem | null> {
  const id = parseMediaSlug(slug)
  const response = await fetch(`http://localhost:5001/api/media`, {
    credentials: 'include',
  })
  if (!response.ok) return null
  const data = await response.json()

  return data.results.find((item: MediaItem) => item.id === id) ?? null
}

async function fetchJellyfinUrl(): Promise<string | null> {
  try {
    const response = await fetch(`http://localhost:5001/api/settings`, {
      credentials: 'include',
    })
    if (!response.ok) return null
    const data = await response.json()
    return data.settings?.auth?.jellyfin?.url ?? null
  } catch {
    return null
  }
}

async function fetchExternalIds(tmdbId: string, isTV: boolean): Promise<{ imdbId?: string } | null> {
  try {
    const type = isTV ? 'tv/shows' : 'movies'
    const response = await fetch(`http://localhost:5001/api/metadata/tmdb/${type}/${tmdbId}/external_ids`, {
      credentials: 'include',
    })
    if (!response.ok) return null
    const data = await response.json()
    return { imdbId: data.imdb_id }
  } catch {
    return null
  }
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

function QualityBadge({ label, value, variant = 'default' }: { label: string; value: string; variant?: 'default' | 'accent' | 'muted' }) {
  const styles = {
    default: 'bg-card border-border text-foreground',
    accent: 'bg-accent/20 border-accent/30 text-accent',
    muted: 'bg-background border-border text-muted',
  }

  return (
    <div className={`rounded-lg border px-4 py-3 ${styles[variant]}`}>
      <p className="text-xs text-muted mb-1">{label}</p>
      <p className="font-medium">{value}</p>
    </div>
  )
}

function isSizeWarning(sizeBytes: number | undefined, resolution: string | undefined): boolean {
  if (!sizeBytes) return false
  const sizeMB = sizeBytes / 1024 / 1024
  const minMB = resolution === '2160p' ? 2000 : resolution === '720p' ? 700 : 1000
  return sizeMB < minMB
}

export default function MediaDetails({ slug }: MediaDetailsProps) {
  const { data: item, isLoading, error } = useQuery({
    queryKey: ['media-item', slug],
    queryFn: () => fetchMediaItem(slug),
  })

  const { data: jellyfinUrl } = useQuery({
    queryKey: ['jellyfin-url'],
    queryFn: fetchJellyfinUrl,
    staleTime: 1000 * 60 * 60,
  })

  const tmdbId = item?.metadata?.id?.replace('relay:', '')
  const isTV = item?.parsed?.season !== undefined

  const { data: externalIds } = useQuery({
    queryKey: ['external-ids', tmdbId, isTV],
    queryFn: () => fetchExternalIds(tmdbId!, isTV),
    enabled: !!tmdbId,
    staleTime: 1000 * 60 * 60,
  })

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div className="h-10 w-10 animate-spin rounded-full border-2 border-accent border-t-transparent" />
      </div>
    )
  }

  if (error || !item) {
    return (
      <div className="min-h-screen p-6">
        <Link to="/library" className="inline-flex items-center gap-2 text-muted hover:text-foreground mb-6">
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
          </svg>
          Back to Library
        </Link>
        <div className="rounded-lg bg-card border border-red-500/30 p-8 text-center">
          <h2 className="text-lg font-semibold text-foreground mb-2">Media Not Found</h2>
          <p className="text-muted">The requested media item could not be found.</p>
        </div>
      </div>
    )
  }

  const { parsed, metadata } = item
  const title = metadata?.title || parsed.title
  const year = metadata?.year || parsed.year
  const posterUrl = getPosterUrl(metadata?.posterPath, 'w500')
  const backdropUrl = metadata?.backdropPath
    ? `https://image.tmdb.org/t/p/w1280${metadata.backdropPath}`
    : null

  return (
    <div className="min-h-screen">
      {/* Backdrop */}
      {backdropUrl && (
        <div className="absolute inset-x-0 top-0 h-[400px] overflow-hidden">
          <img
            src={backdropUrl}
            alt=""
            className="w-full h-full object-cover"
          />
          <div className="absolute inset-0 bg-gradient-to-b from-background/60 via-background/80 to-background" />
        </div>
      )}

      <div className="relative z-10 p-6">
        {/* Back Button */}
        <Link to="/library" className="inline-flex items-center gap-2 text-muted hover:text-foreground mb-6 transition-colors">
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
          </svg>
          Back to Library
        </Link>

        {/* Main Content */}
        <div className="flex flex-col md:flex-row gap-8">
          {/* Poster */}
          <div className="flex-shrink-0">
            <div className="w-64 aspect-[2/3] rounded-lg overflow-hidden bg-card shadow-2xl">
              <img
                src={posterUrl}
                alt={title}
                className="w-full h-full object-cover"
              />
            </div>
          </div>

          {/* Info */}
          <div className="flex-1 min-w-0">
            <div className="mb-6">
              <h1 className="text-3xl font-bold text-foreground mb-2">
                {title}
                {year && <span className="text-muted ml-3">({year})</span>}
              </h1>

              {/* Quick badges */}
              <div className="flex flex-wrap gap-2 mb-4">
                {parsed.resolution && (
                  <span className="px-2 py-1 text-xs font-medium rounded bg-accent/20 text-accent uppercase">
                    {parsed.resolution}
                  </span>
                )}
                {parsed.hdr && (
                  <span className="px-2 py-1 text-xs font-medium rounded bg-yellow-500/20 text-yellow-400 uppercase">
                    {parsed.hdr}
                  </span>
                )}
                {parsed.source && (
                  <span className="px-2 py-1 text-xs font-medium rounded bg-card text-foreground uppercase">
                    {parsed.source}
                  </span>
                )}
                {parsed.proper && (
                  <span className="px-2 py-1 text-xs font-medium rounded bg-green-500/20 text-green-400">
                    PROPER
                  </span>
                )}
                {parsed.repack && (
                  <span className="px-2 py-1 text-xs font-medium rounded bg-blue-500/20 text-blue-400">
                    REPACK
                  </span>
                )}
              </div>

              {/* Rating */}
              {metadata?.rating && metadata.rating > 0 && (
                <div className="flex items-center gap-2 mb-4">
                  <span className="text-yellow-400 text-xl">★</span>
                  <span className="text-xl font-semibold text-foreground">{metadata.rating.toFixed(1)}</span>
                  <span className="text-muted">/ 10</span>
                </div>
              )}

              {/* Overview */}
              {metadata?.overview && (
                <p className="text-muted leading-relaxed mb-6">{metadata.overview}</p>
              )}

              {/* Action Buttons */}
              <div className="flex flex-wrap gap-3 mb-8">
                {jellyfinUrl && metadata?.id && (
                  <a
                    href={`${jellyfinUrl}/web/index.html#!/search.html?query=${encodeURIComponent(title)}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-2 px-6 py-3 rounded-lg bg-[#00a4dc] text-white font-medium hover:bg-[#0090c4] transition-colors"
                  >
                    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="currentColor">
                      <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z" />
                    </svg>
                    Open in Jellyfin
                  </a>
                )}
                {metadata?.id && (metadata.provider === 'tmdb' || metadata.provider === 'relay') && (
                  <a
                    href={`https://www.themoviedb.org/${parsed.season !== undefined ? 'tv' : 'movie'}/${metadata.id.replace('relay:', '')}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-2 px-4 py-3 rounded-lg bg-[#01b4e4] text-white font-medium hover:bg-[#019fc7] transition-colors"
                  >
                    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="currentColor">
                      <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z" />
                    </svg>
                    TMDB
                  </a>
                )}
                {externalIds?.imdbId && (
                  <a
                    href={`https://www.imdb.com/title/${externalIds.imdbId}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-2 px-4 py-3 rounded-lg bg-[#f5c518] text-black font-medium hover:bg-[#e6b800] transition-colors"
                  >
                    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="currentColor">
                      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6zm4 18H6V4h7v5h5v11z" />
                    </svg>
                    IMDB
                  </a>
                )}
              </div>
            </div>

            {/* Technical Details */}
            <div className="mb-8">
              <h2 className="text-lg font-semibold text-foreground mb-4">Technical Details</h2>
              <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3">
                {parsed.resolution && (
                  <QualityBadge label="Resolution" value={parsed.resolution.toUpperCase()} variant="accent" />
                )}
                {parsed.videoCodec && (
                  <QualityBadge label="Video Codec" value={parsed.videoCodec.toUpperCase()} />
                )}
                {parsed.audioCodec && (
                  <QualityBadge label="Audio Codec" value={parsed.audioCodec.toUpperCase()} />
                )}
                {parsed.source && (
                  <QualityBadge label="Source" value={parsed.source.toUpperCase()} />
                )}
                {parsed.hdr && (
                  <QualityBadge label="HDR" value={parsed.hdr.toUpperCase()} variant="accent" />
                )}
                {parsed.releaseGroup && (
                  <QualityBadge label="Release Group" value={parsed.releaseGroup} />
                )}
                {parsed.season !== undefined && (
                  <QualityBadge label="Season" value={String(parsed.season).padStart(2, '0')} />
                )}
                {parsed.episode !== undefined && (
                  <QualityBadge label="Episode" value={String(parsed.episode).padStart(2, '0')} />
                )}
              </div>
            </div>

            {/* File Info */}
            <div>
              <h2 className="text-lg font-semibold text-foreground mb-4">File Information</h2>
              {isSizeWarning(item.size, parsed.resolution) && (
                <div className="rounded-lg bg-orange-500/10 border border-orange-500/30 p-3 mb-4 flex items-start gap-3">
                  <svg className="w-5 h-5 text-orange-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                  </svg>
                  <div>
                    <p className="text-sm font-medium text-orange-400">File size seems low</p>
                    <p className="text-xs text-orange-400/70 mt-0.5">
                      This file may be a low quality encode or incomplete download.
                    </p>
                  </div>
                </div>
              )}
              <div className="rounded-lg bg-card border border-border p-4">
                <div className="grid gap-3">
                  {item.size && (
                    <div>
                      <p className="text-xs text-muted mb-1">Size</p>
                      <p className="text-sm text-foreground font-mono">{formatFileSize(item.size)}</p>
                    </div>
                  )}
                  <div>
                    <p className="text-xs text-muted mb-1">Filename</p>
                    <p className="text-sm text-foreground font-mono break-all">{item.filename}</p>
                  </div>
                  <div>
                    <p className="text-xs text-muted mb-1">Path</p>
                    <p className="text-sm text-foreground font-mono break-all">{item.path}</p>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
