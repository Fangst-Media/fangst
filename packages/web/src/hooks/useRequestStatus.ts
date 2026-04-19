import { useQuery } from '@tanstack/react-query'
import { apiFetch } from '../lib/api-client'

export type RequestStatus =
  | 'pending_approval'
  | 'approved'
  | 'searching'
  | 'found'
  | 'fetching_nzb'
  | 'nzb_downloaded'
  | 'sent_to_downloader'
  | 'downloading'
  | 'download_complete'
  | 'importing'
  | 'completed'
  | 'rejected'
  | 'failed'

export type DownloadRecord = {
  id: string
  releaseName: string
  status: 'fetching' | 'downloading' | 'complete' | 'failed' | 'cancelled' | 'replaced'
  resolution: string | null
  source: string | null
  hdr: string | null
  qualityScore: number | null
  createdAt: string
  completedAt: string | null
}

export type MediaRequest = {
  id: string
  title: string
  type: 'movie' | 'tv' | 'book'
  status: RequestStatus
  tmdbId: string | null
  openLibraryId: string | null
  format: 'ebook' | 'audiobook' | null
  nzoId: string | null
  releaseName: string | null
  qualityProfileId: string | null
  mode: 'auto' | 'manual'
  season: number | null
  posterPath: string | null
  createdAt: string
  downloads?: DownloadRecord[]
}

type RequestsResponse = {
  requests: MediaRequest[]
}

async function fetchRequestByTmdbId(tmdbId: string, type: 'movie' | 'tv'): Promise<MediaRequest | null> {
  try {
    const data = await apiFetch<RequestsResponse>(`/api/requests?tmdbId=${tmdbId}&type=${type}`)
    return data.requests[0] ?? null
  } catch {
    return null
  }
}

async function fetchBookRequest(openLibraryId: string, format: 'ebook' | 'audiobook'): Promise<MediaRequest | null> {
  try {
    const data = await apiFetch<RequestsResponse>(`/api/requests?openLibraryId=${openLibraryId}&type=book`)
    return data.requests.find((r) => r.format === format) ?? null
  } catch {
    return null
  }
}

const ACTIVE_STATUSES: RequestStatus[] = [
  'pending_approval',
  'approved',
  'searching',
  'found',
  'fetching_nzb',
  'nzb_downloaded',
  'sent_to_downloader',
  'downloading',
  'download_complete',
  'importing',
]

export function useRequestStatus(tmdbId: string, type: 'movie' | 'tv') {
  const query = useQuery({
    queryKey: ['request-status', tmdbId, type],
    queryFn: () => fetchRequestByTmdbId(tmdbId, type),
    staleTime: 1000 * 5,
    refetchInterval: (query) => {
      const request = query.state.data
      if (request && ACTIVE_STATUSES.includes(request.status)) {
        return 1000 * 10
      }
      return false
    },
  })

  return {
    request: query.data ?? null,
    isLoading: query.isLoading,
    refetch: query.refetch,
  }
}

export function useBookRequestStatus(openLibraryId: string, format: 'ebook' | 'audiobook') {
  const query = useQuery({
    queryKey: ['request-status', openLibraryId, 'book', format],
    queryFn: () => fetchBookRequest(openLibraryId, format),
    staleTime: 1000 * 5,
    refetchInterval: (query) => {
      const request = query.state.data
      if (request && ACTIVE_STATUSES.includes(request.status)) {
        return 1000 * 10
      }
      return false
    },
  })

  return {
    request: query.data ?? null,
    isLoading: query.isLoading,
    refetch: query.refetch,
  }
}
