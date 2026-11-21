import { Link } from '@tanstack/react-router'
import { type ReactNode, useState } from 'react'
import { type EventsStatus, useJobEvents } from '../../hooks/useJobEvents'
import { setApiKey, useApiKey } from '../../lib/dl-api'
import { ApiKeyForm } from './ApiKeyForm'

const eventsLabel: Record<EventsStatus, { label: string; dot: string }> = {
  connecting: { label: 'Connecting', dot: 'bg-yellow-400 animate-pulse' },
  live: { label: 'Live', dot: 'bg-green-400' },
  offline: { label: 'Offline', dot: 'bg-red-400' },
}

/**
 * Shell for the download daemon pages. It does not use the old app's auth or connection
 * check, since the daemon is a separate service with its own API key.
 */
export function DownloadsLayout({ children }: { children: ReactNode }) {
  const apiKey = useApiKey()
  const events = useJobEvents(apiKey)
  const [editingKey, setEditingKey] = useState(false)
  const status = eventsLabel[events]

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b border-border bg-card/50">
        <nav className="mx-auto flex h-16 max-w-7xl items-center justify-between px-6">
          <div className="flex items-center gap-6">
            <Link to="/downloads" className="text-xl font-bold text-accent">
              Fangst
            </Link>
            <Link
              to="/downloads"
              className="text-sm font-medium text-foreground/60 hover:text-foreground"
              activeProps={{ className: 'text-foreground' }}
              activeOptions={{ exact: true }}
            >
              Queue
            </Link>
          </div>
          <div className="flex items-center gap-4">
            {apiKey !== null && (
              <span className="flex items-center gap-2 text-xs text-muted" data-testid="events">
                <span className={`h-2 w-2 rounded-full ${status.dot}`} />
                {status.label}
              </span>
            )}
            {apiKey !== null && (
              <>
                <button
                  type="button"
                  onClick={() => setEditingKey(true)}
                  className="text-sm text-foreground/60 hover:text-foreground"
                >
                  Change key
                </button>
                <button
                  type="button"
                  onClick={() => setApiKey(null)}
                  className="text-sm text-foreground/60 hover:text-foreground"
                >
                  Forget key
                </button>
              </>
            )}
          </div>
        </nav>
      </header>
      <main className="mx-auto max-w-7xl px-6 py-8">
        {apiKey === null ? (
          <ApiKeyForm />
        ) : editingKey ? (
          <ApiKeyForm onDone={() => setEditingKey(false)} />
        ) : (
          children
        )}
      </main>
    </div>
  )
}
