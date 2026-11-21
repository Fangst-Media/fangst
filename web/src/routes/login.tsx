import { useState } from 'react'
import { useAuth } from '../lib/auth'

type LoginProps = {
  onSuccess: () => void
}

export default function Login({ onSuccess }: LoginProps) {
  const { login } = useAuth()
  const [username, setUsername] = useState('')
  const [error, setError] = useState('')
  const [isLoading, setIsLoading] = useState(false)

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setError('')
    setIsLoading(true)

    try {
      await login('jellyfin', username, '')
      onSuccess()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Login failed')
    } finally {
      setIsLoading(false)
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-sm">
        <div className="text-center mb-8">
          <h1 className="text-4xl font-bold text-accent mb-2">Jæger</h1>
          <p className="text-muted">Sign in to continue</p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          {error && (
            <div className="rounded-md bg-red-500/10 border border-red-500/20 p-3">
              <p className="text-sm text-red-400">{error}</p>
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

          <button
            type="submit"
            disabled={isLoading || !username}
            className="w-full rounded-md bg-accent px-4 py-2.5 font-medium text-white hover:bg-accent/90 focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-2 focus:ring-offset-background disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isLoading ? 'Signing in...' : 'Sign in'}
          </button>
        </form>

        <div className="mt-6 p-4 rounded-md bg-card border border-border">
          <p className="text-xs text-muted text-center mb-2">Mock Auth Mode</p>
          <p className="text-xs text-muted text-center">
            Use <span className="text-accent font-medium">admin</span> for admin access, any other
            name for regular user
          </p>
        </div>
      </div>
    </div>
  )
}
