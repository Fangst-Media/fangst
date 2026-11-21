import type { BookDetails } from '@fangst/shared/schemas/metadata'
import type { ExternalLink } from '@fangst/shared/types/settings'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { DownloadStatus } from '../components/DownloadStatus'
import { useBookRequestStatus } from '../hooks/useRequestStatus'
import { apiFetch, apiPost, bookLanes } from '../lib/api-client'

function RequestButton({
  label,
  book,
  format,
}: {
  label: string
  book: BookDetails
  format: 'ebook' | 'audiobook'
}) {
  const queryClient = useQueryClient()
  const { request: formatRequest, isLoading: isLoadingStatus } = useBookRequestStatus(
    book.id,
    format
  )

  const mutation = useMutation({
    mutationFn: () =>
      apiPost<{ success: boolean }>('/api/requests', {
        title: book.title,
        year: book.year,
        type: 'book',
        openLibraryId: book.id,
        posterPath: book.cover,
        overview: book.overview,
        author: book.author,
        format,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['request-status', book.id] })
      queryClient.invalidateQueries({ queryKey: ['requests'] })
    },
  })

  if (isLoadingStatus) {
    return <div className="h-10 w-40 animate-pulse rounded-md bg-card" />
  }

  if (formatRequest) {
    return <DownloadStatus status={formatRequest.status} />
  }

  return (
    <button
      type="button"
      onClick={() => mutation.mutate()}
      disabled={mutation.isPending}
      className="flex items-center gap-2 rounded-md bg-accent px-5 py-2 text-sm font-semibold text-white hover:bg-accent/90 transition-colors disabled:opacity-50"
    >
      {mutation.isPending ? (
        <>
          <div className="h-4 w-4 animate-spin rounded-full border-2 border-white border-t-transparent" />
          Requesting...
        </>
      ) : (
        label
      )}
    </button>
  )
}

function resolveUrl(template: string, vars: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (_, key: string) =>
    vars[key] !== undefined ? encodeURIComponent(vars[key]) : ''
  )
}

export default function BookDetail({ openLibraryId }: { openLibraryId: string }) {
  const { data: book, isLoading } = useQuery({
    queryKey: ['book-details', openLibraryId],
    queryFn: () => bookLanes.getByWorkId(openLibraryId),
  })

  const { data: linksData } = useQuery({
    queryKey: ['external-links'],
    queryFn: () => apiFetch<{ links: ExternalLink[] }>('/api/settings/external-links'),
    staleTime: 1000 * 60 * 30,
  })

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
      </div>
    )
  }

  if (!book) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <p className="text-muted">Book not found</p>
      </div>
    )
  }

  const bookLinks = (linksData?.links ?? []).filter((l) => l.mediaTypes.includes('book'))
  const linkVars: Record<string, string> = {
    title: book.title,
    author: book.author ?? '',
    year: book.year?.toString() ?? '',
    openLibraryId,
  }

  return (
    <div className="min-h-screen">
      {/* Hero — blurred cover as backdrop */}
      <div className="-mt-16 relative h-[50vh] min-h-[350px] w-full overflow-hidden">
        {book.cover ? (
          <img
            src={book.cover}
            alt=""
            className="absolute inset-0 w-full h-full object-cover scale-110 blur-2xl opacity-40"
          />
        ) : (
          <div className="absolute inset-0 bg-card" />
        )}
        <div className="absolute inset-0 bg-gradient-to-t from-background via-background/60 to-background/30" />
      </div>

      {/* Content */}
      <div className="relative -mt-48 px-6 pb-12 max-w-5xl mx-auto">
        <div className="flex flex-col md:flex-row gap-8">
          {/* Cover */}
          <div className="shrink-0">
            {book.cover ? (
              <img
                src={book.cover}
                alt={book.title}
                className="w-48 md:w-56 rounded-lg shadow-2xl"
              />
            ) : (
              <div className="w-48 md:w-56 aspect-[2/3] rounded-lg bg-card border border-border flex items-center justify-center">
                <svg
                  aria-hidden="true"
                  className="w-12 h-12 text-muted"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={1}
                    d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253"
                  />
                </svg>
              </div>
            )}
          </div>

          {/* Info */}
          <div className="flex-1 min-w-0">
            <h1 className="text-3xl md:text-4xl font-bold text-foreground mb-2">{book.title}</h1>

            <div className="flex flex-wrap items-center gap-3 mb-4 text-sm text-muted">
              {book.author && <span className="text-foreground/70">{book.author}</span>}
              {book.year && <span>{book.year}</span>}
              {book.numberOfPages && <span>{book.numberOfPages} pages</span>}
            </div>

            {book.subjects && book.subjects.length > 0 && (
              <div className="flex flex-wrap gap-1.5 mb-4">
                {book.subjects.slice(0, 5).map((subject) => (
                  <span
                    key={subject}
                    className="px-2 py-0.5 rounded bg-foreground/10 text-xs text-muted"
                  >
                    {subject}
                  </span>
                ))}
              </div>
            )}

            {book.overview && (
              <p className="text-foreground/80 leading-relaxed mb-6 max-w-2xl">{book.overview}</p>
            )}

            {/* Request buttons */}
            <div className="flex flex-wrap items-center gap-4">
              <RequestButton label="Request Ebook" book={book} format="ebook" />
              <RequestButton label="Request Audiobook" book={book} format="audiobook" />
            </div>

            {/* External links */}
            {bookLinks.length > 0 && (
              <div className="flex flex-wrap items-center gap-3 mt-8">
                {bookLinks.map((link) => (
                  <a
                    key={link.id}
                    href={resolveUrl(link.urlTemplate, linkVars)}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="px-3 py-1.5 rounded-md bg-card border border-border text-sm text-muted hover:text-foreground transition-colors"
                  >
                    {link.name}
                  </a>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
