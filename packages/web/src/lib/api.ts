const API_BASE_URL = 'http://localhost:5001'

export type MediaItem = {
  path: string
  filename: string
  parsed: {
    title: string
    year?: number
    season?: number
    episode?: number
    resolution?: string
    source?: string
    videoCodec?: string
    audioCodec?: string
    hdr?: string
    releaseGroup?: string
    proper?: boolean
    repack?: boolean
  }
  metadata?: {
    id: string
    title: string
    year?: number
    overview?: string
    posterPath?: string
    backdropPath?: string
    rating?: number
    provider: string
  }
}

export type MediaResponse = {
  results: MediaItem[]
}

export type ScanResponse = MediaResponse & {
  mockFilesystem: boolean
  providers: string[]
  scannedPath: string
  sync: {
    added: number
    updated: number
    removed: number
  }
}

export async function getMedia(): Promise<MediaResponse> {
  const response = await fetch(`${API_BASE_URL}/api/media`, {
    credentials: 'include',
  })
  if (!response.ok) {
    throw new Error(`Failed to fetch media: ${response.statusText}`)
  }
  return response.json()
}

export async function triggerScan(): Promise<ScanResponse> {
  const response = await fetch(`${API_BASE_URL}/api/media/scan`, {
    method: 'POST',
    credentials: 'include',
  })
  if (!response.ok) {
    throw new Error(`Failed to scan media: ${response.statusText}`)
  }
  return response.json()
}

export function getPosterUrl(posterPath?: string, size: 'w185' | 'w342' | 'w500' = 'w342'): string {
  if (!posterPath) {
    return '/placeholder-poster.png'
  }

  if (posterPath.startsWith('http')) {
    return posterPath
  }

  return `https://image.tmdb.org/t/p/${size}${posterPath}`
}
