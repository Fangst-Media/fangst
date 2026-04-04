import { useState, useEffect } from 'react'
import { useAuth } from '../lib/auth'

const API_BASE_URL = 'http://localhost:5001'

type LoginProps = {
  onSuccess: () => void
}

type ProvidersResponse = {
  providers: string[]
  registrationEnabled: boolean
}

export default function Login({ onSuccess }: LoginProps) {
  const { login, register } = useAuth()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const [isLoading, setIsLoading] = useState(false)
  const [isRegistering, setIsRegistering] = useState(false)
  const [providers, setProviders] = useState<string[]>([])
  const [registrationEnabled, setRegistrationEnabled] = useState(false)
  const [selectedProvider, setSelectedProvider] = useState<string>('')

  useEffect(() => {
    fetchProviders()
  }, [])

  async function fetchProviders() {
    try {
      const response = await fetch(`${API_BASE_URL}/api/auth/providers`)
      const data: ProvidersResponse = await response.json()
      setProviders(data.providers)
      setRegistrationEnabled(data.registrationEnabled)
      if (data.providers.length > 0) {
        setSelectedProvider(data.providers[0]!)
      }
    } catch {
      setProviders(['local'])
      setSelectedProvider('local')
    }
  }

  const needsPassword = selectedProvider !== 'mock'

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setError('')
    setIsLoading(true)

    try {
      if (isRegistering) {
        await register(username, password)
      } else {
        await login(selectedProvider, username, password)
      }
      onSuccess()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Authentication failed')
    } finally {
      setIsLoading(false)
    }
  }

  const providerLabel: Record<string, string> = {
    local: 'Local',
    jellyfin: 'Jellyfin',
    plex: 'Plex',
    mock: 'Mock',
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-sm">
        <div className="text-center mb-8">
          <h1 className="text-4xl font-bold text-accent mb-2">Jæger</h1>
          <p className="text-muted">
            {isRegistering ? 'Create an account' : 'Sign in to continue'}
          </p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          {error && (
            <div className="rounded-md bg-red-500/10 border border-red-500/20 p-3">
              <p className="text-sm text-red-400">{error}</p>
            </div>
          )}

          {!isRegistering && providers.length > 1 && (
            <div>
              <label htmlFor="provider" className="block text-sm font-medium text-foreground mb-1.5">
                Sign in with
              </label>
              <select
                id="provider"
                value={selectedProvider}
                onChange={(e) => {
                  setSelectedProvider(e.target.value)
                  setError('')
                }}
                className="w-full rounded-md bg-card border border-border px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-accent"
              >
                {providers.map((p) => (
                  <option key={p} value={p}>
                    {providerLabel[p] ?? p}
                  </option>
                ))}
              </select>
            </div>
          )}

          <div>
            <label htmlFor="username" className="block text-sm font-medium text-foreground mb-1.5">
              Username
            </label>
            <input
              id="username"
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="w-full rounded-md bg-card border border-border px-3 py-2 text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent"
              placeholder="Enter username"
              required
              autoComplete="username"
            />
          </div>

          {needsPassword && (
            <div>
              <label htmlFor="password" className="block text-sm font-medium text-foreground mb-1.5">
                Password
              </label>
              <input
                id="password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="w-full rounded-md bg-card border border-border px-3 py-2 text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent"
                placeholder={isRegistering ? 'Min 8 characters' : 'Enter password'}
                required
                autoComplete={isRegistering ? 'new-password' : 'current-password'}
              />
            </div>
          )}

          <button
            type="submit"
            disabled={isLoading || !username || (needsPassword && !password)}
            className="w-full rounded-md bg-accent px-4 py-2.5 font-medium text-white hover:bg-accent/90 focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-2 focus:ring-offset-background disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isLoading
              ? isRegistering
                ? 'Creating account...'
                : 'Signing in...'
              : isRegistering
                ? 'Create account'
                : 'Sign in'}
          </button>
        </form>

        {registrationEnabled && selectedProvider === 'local' && (
          <div className="mt-4 text-center">
            <button
              type="button"
              onClick={() => {
                setIsRegistering(!isRegistering)
                setError('')
              }}
              className="text-sm text-accent hover:text-accent/80 transition-colors"
            >
              {isRegistering
                ? 'Already have an account? Sign in'
                : "Don't have an account? Create one"}
            </button>
          </div>
        )}

        {selectedProvider === 'mock' && (
          <div className="mt-6 p-4 rounded-md bg-card border border-border">
            <p className="text-xs text-muted text-center mb-2">Mock Auth Mode</p>
            <p className="text-xs text-muted text-center">
              Use <span className="text-accent font-medium">admin</span> for admin access, any other name for regular user
            </p>
          </div>
        )}
      </div>
    </div>
  )
}
