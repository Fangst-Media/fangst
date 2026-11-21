import { createContext, useContext, useState, useEffect, useCallback, type ReactNode } from 'react'

const API_BASE_URL = 'http://localhost:5001'
const HEALTH_CHECK_INTERVAL = 5000
const RECONNECT_INTERVAL = 3000

type ConnectionStatus = 'connected' | 'disconnected' | 'reconnecting'

type ConnectionContextType = {
  status: ConnectionStatus
  lastError: string | null
  retry: () => void
}

const ConnectionContext = createContext<ConnectionContextType | null>(null)

export function ConnectionProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<ConnectionStatus>('connected')
  const [lastError, setLastError] = useState<string | null>(null)
  const [retryCount, setRetryCount] = useState(0)

  const checkConnection = useCallback(async () => {
    try {
      const controller = new AbortController()
      const timeout = setTimeout(() => controller.abort(), 5000)

      const response = await fetch(`${API_BASE_URL}/health`, {
        method: 'GET',
        signal: controller.signal,
      })

      clearTimeout(timeout)

      if (response.ok) {
        if (status !== 'connected') {
          setStatus('connected')
          setLastError(null)
          setRetryCount(0)
        }
        return true
      }
      throw new Error(`Server returned ${response.status}`)
    } catch (error) {
      const message =
        error instanceof Error
          ? error.name === 'AbortError'
            ? 'Connection timeout'
            : error.message
          : 'Connection failed'

      if (status === 'connected') {
        setStatus('disconnected')
        setLastError(message)
      }
      return false
    }
  }, [status])

  const retry = useCallback(() => {
    setStatus('reconnecting')
    setRetryCount((c) => c + 1)
    checkConnection()
  }, [checkConnection])

  useEffect(() => {
    checkConnection()

    const interval = setInterval(() => {
      if (status === 'disconnected' || status === 'reconnecting') {
        checkConnection()
      }
    }, RECONNECT_INTERVAL)

    return () => clearInterval(interval)
  }, [status, checkConnection])

  useEffect(() => {
    if (status !== 'connected') return

    const interval = setInterval(checkConnection, HEALTH_CHECK_INTERVAL)
    return () => clearInterval(interval)
  }, [status, checkConnection])

  // Listen for fetch errors globally
  useEffect(() => {
    const handleError = (event: PromiseRejectionEvent) => {
      if (
        event.reason instanceof TypeError &&
        (event.reason.message.includes('NetworkError') ||
          event.reason.message.includes('Failed to fetch') ||
          event.reason.message.includes('Network request failed'))
      ) {
        if (status === 'connected') {
          setStatus('disconnected')
          setLastError('Network error')
        }
      }
    }

    window.addEventListener('unhandledrejection', handleError)
    return () => window.removeEventListener('unhandledrejection', handleError)
  }, [status])

  return (
    <ConnectionContext.Provider value={{ status, lastError, retry }}>
      {children}
      {status !== 'connected' && <ConnectionDialog status={status} lastError={lastError} retry={retry} retryCount={retryCount} />}
    </ConnectionContext.Provider>
  )
}

function ConnectionDialog({
  status,
  lastError,
  retry,
  retryCount,
}: {
  status: ConnectionStatus
  lastError: string | null
  retry: () => void
  retryCount: number
}) {
  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/70 backdrop-blur-sm">
      <div className="w-full max-w-sm m-4 rounded-xl bg-card border border-border shadow-2xl overflow-hidden">
        <div className="p-6 text-center">
          {status === 'reconnecting' ? (
            <div className="h-12 w-12 mx-auto mb-4 animate-spin rounded-full border-3 border-accent border-t-transparent" />
          ) : (
            <div className="h-12 w-12 mx-auto mb-4 rounded-full bg-red-500/10 flex items-center justify-center">
              <svg
                className="w-6 h-6 text-red-400"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                />
              </svg>
            </div>
          )}

          <h2 className="text-lg font-semibold text-foreground mb-2">
            {status === 'reconnecting' ? 'Reconnecting...' : 'Connection Lost'}
          </h2>

          <p className="text-sm text-muted mb-4">
            {status === 'reconnecting'
              ? 'Attempting to reconnect to the server...'
              : 'Unable to connect to the server. Please check that the API is running.'}
          </p>

          {lastError && (
            <p className="text-xs text-red-400 mb-4 px-3 py-2 rounded bg-red-500/10">
              {lastError}
            </p>
          )}

          {retryCount > 0 && (
            <p className="text-xs text-muted mb-4">
              Retry attempts: {retryCount}
            </p>
          )}

          <button
            type="button"
            onClick={retry}
            disabled={status === 'reconnecting'}
            className="w-full rounded-md bg-accent px-4 py-2.5 text-sm font-medium text-white hover:bg-accent/90 transition-colors disabled:opacity-50"
          >
            {status === 'reconnecting' ? 'Reconnecting...' : 'Retry Now'}
          </button>
        </div>
      </div>
    </div>
  )
}

export function useConnection() {
  const context = useContext(ConnectionContext)
  if (!context) {
    throw new Error('useConnection must be used within a ConnectionProvider')
  }
  return context
}
