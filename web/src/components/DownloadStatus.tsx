import type { RequestStatus } from '../hooks/useRequestStatus'

type DownloadStatusProps = {
  status: RequestStatus
  jellyfinUrl?: string
}

const statusConfig: Record<RequestStatus, { label: string; color: string; animate?: boolean }> = {
  pending_approval: { label: 'Awaiting Approval', color: 'text-yellow-400' },
  approved: { label: 'Approved', color: 'text-blue-400' },
  searching: { label: 'Searching...', color: 'text-blue-400', animate: true },
  found: { label: 'Release Found', color: 'text-blue-400' },
  fetching_nzb: { label: 'Fetching NZB...', color: 'text-blue-400', animate: true },
  nzb_downloaded: { label: 'NZB Ready', color: 'text-blue-400' },
  sent_to_downloader: { label: 'Sent to Downloader', color: 'text-blue-400' },
  downloading: { label: 'Downloading...', color: 'text-accent', animate: true },
  download_complete: { label: 'Download Complete', color: 'text-green-400' },
  importing: { label: 'Importing...', color: 'text-green-400', animate: true },
  completed: { label: 'Available', color: 'text-green-400' },
  rejected: { label: 'Rejected', color: 'text-red-400' },
  failed: { label: 'Failed', color: 'text-red-400' },
}

export function DownloadStatus({ status, jellyfinUrl }: DownloadStatusProps) {
  const config = statusConfig[status]

  return (
    <div className="flex items-center gap-3">
      <div className="flex items-center gap-2">
        {config.animate && (
          <div className="h-2 w-2 rounded-full bg-current animate-pulse" />
        )}
        {!config.animate && (
          <div className={`h-2 w-2 rounded-full ${
            status === 'completed' ? 'bg-green-400' :
            status === 'failed' || status === 'rejected' ? 'bg-red-400' :
            'bg-current'
          }`} />
        )}
        <span className={`text-sm font-medium ${config.color}`}>
          {config.label}
        </span>
      </div>

      {status === 'completed' && jellyfinUrl && (
        <a
          href={jellyfinUrl}
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-accent text-sm font-medium text-white hover:bg-accent/90 transition-colors"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          Watch
        </a>
      )}
    </div>
  )
}
