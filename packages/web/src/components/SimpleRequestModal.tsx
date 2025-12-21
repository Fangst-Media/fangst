import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'

type SearchResult = {
  id: string
  title: string
  year?: number
  overview?: string
  posterPath?: string
  type: 'movie' | 'tv'
}

type SimpleRequestModalProps = {
  media: SearchResult
  onClose: () => void
  defaultQuality?: string
}

async function createRequest(data: {
  title: string
  year?: number
  type: 'movie' | 'tv'
  tmdbId?: string
  posterPath?: string
  overview?: string
  minQuality?: string | null
}): Promise<{ success: boolean }> {
  const response = await fetch('/api/requests', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!response.ok) {
    const error = await response.json()
    throw new Error(error.error || 'Failed to create request')
  }
  return response.json()
}

export function SimpleRequestModal({ media, onClose, defaultQuality = '1080p' }: SimpleRequestModalProps) {
  const [quality, setQuality] = useState(defaultQuality)
  const [successMessage, setSuccessMessage] = useState<string | null>(null)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const queryClient = useQueryClient()

  const requestMutation = useMutation({
    mutationFn: async () => {
      return createRequest({
        title: media.title,
        year: media.year,
        type: media.type,
        tmdbId: media.id,
        posterPath: media.posterPath,
        overview: media.overview,
        minQuality: quality === 'any' ? null : quality,
      })
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['requests'] })
      setSuccessMessage('Request created!')
      setTimeout(() => {
        setSuccessMessage(null)
        onClose()
      }, 1500)
    },
    onError: (error: Error) => {
      setErrorMessage(error.message)
      setTimeout(() => setErrorMessage(null), 5000)
    },
  })

  function getPosterUrl(path?: string) {
    if (!path) return '/placeholder-poster.png'
    return `https://image.tmdb.org/t/p/w185${path}`
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={onClose}
      onKeyDown={(e) => e.key === 'Escape' && onClose()}
      role="dialog"
      aria-modal="true"
      aria-labelledby="simple-request-title"
    >
      <div
        className="relative w-full max-w-sm m-4 rounded-xl bg-card border border-border shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        {/* Header with poster */}
        <div className="flex gap-4 p-4 border-b border-border">
          <img
            src={getPosterUrl(media.posterPath)}
            alt={media.title}
            className="w-16 h-24 rounded object-cover bg-background"
          />
          <div className="flex-1 min-w-0">
            <h2 id="simple-request-title" className="font-semibold text-foreground line-clamp-2">
              {media.title}
            </h2>
            {media.year && <p className="text-sm text-muted mt-0.5">({media.year})</p>}
            <span className="inline-block mt-2 px-2 py-0.5 text-xs rounded bg-accent/10 text-accent uppercase">
              {media.type}
            </span>
          </div>
        </div>

        {/* Messages */}
        {successMessage && (
          <div className="mx-4 mt-4 px-4 py-3 rounded-md bg-green-500/10 border border-green-500/20 text-green-400 text-sm">
            {successMessage}
          </div>
        )}
        {errorMessage && (
          <div className="mx-4 mt-4 px-4 py-3 rounded-md bg-red-500/10 border border-red-500/20 text-red-400 text-sm">
            {errorMessage}
          </div>
        )}

        {/* Quality selector */}
        <div className="p-4">
          <label htmlFor="quality-select" className="block text-sm font-medium text-foreground mb-2">
            Preferred Quality
          </label>
          <select
            id="quality-select"
            value={quality}
            onChange={(e) => setQuality(e.target.value)}
            className="w-full rounded-md bg-background border border-border px-3 py-2.5 text-foreground focus:outline-none focus:ring-2 focus:ring-accent"
          >
            <option value="2160p">4K (2160p)</option>
            <option value="1080p">1080p</option>
            <option value="720p">720p</option>
            <option value="any">Any Quality</option>
          </select>
          <p className="mt-2 text-xs text-muted">
            We'll find the best available release at or above this quality.
          </p>
        </div>

        {/* Actions */}
        <div className="flex gap-3 p-4 pt-0">
          <button
            type="button"
            onClick={onClose}
            className="flex-1 rounded-md px-4 py-2.5 text-sm font-medium text-muted border border-border hover:text-foreground hover:border-foreground/30 transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => requestMutation.mutate()}
            disabled={requestMutation.isPending || !!successMessage}
            className="flex-1 rounded-md bg-accent px-4 py-2.5 text-sm font-medium text-white hover:bg-accent/90 transition-colors disabled:opacity-50"
          >
            {requestMutation.isPending ? (
              <span className="flex items-center justify-center gap-2">
                <div className="h-4 w-4 animate-spin rounded-full border-2 border-white border-t-transparent" />
                Requesting...
              </span>
            ) : (
              'Request'
            )}
          </button>
        </div>
      </div>
    </div>
  )
}
