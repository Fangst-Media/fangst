import { createContext, useContext, useState, useEffect, type ReactNode } from 'react'

const API_BASE_URL = 'http://localhost:5001'

type RequestMode = 'simple' | 'detailed'

type User = {
  id: string
  name: string
  role: 'member' | 'admin'
  requestMode: RequestMode
  canChooseRequestMode: boolean
}

type AuthContextType = {
  user: User | null
  isLoading: boolean
  isAdmin: boolean
  requestMode: RequestMode
  canChooseRequestMode: boolean
  setRequestMode: (mode: RequestMode) => Promise<void>
  login: (provider: string, username: string, password: string) => Promise<void>
  logout: () => Promise<void>
}

const AuthContext = createContext<AuthContextType | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null)
  const [isLoading, setIsLoading] = useState(true)

  useEffect(() => {
    checkAuth()
  }, [])

  async function checkAuth() {
    try {
      const response = await fetch(`${API_BASE_URL}/api/auth/me`, {
        credentials: 'include',
      })
      const data = await response.json()
      setUser(data.user)
    } catch {
      setUser(null)
    } finally {
      setIsLoading(false)
    }
  }

  async function login(provider: string, username: string, password: string) {
    const response = await fetch(`${API_BASE_URL}/api/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'include',
      body: JSON.stringify({ provider, username, password }),
    })

    if (!response.ok) {
      const data = await response.json()
      throw new Error(data.error || 'Login failed')
    }

    const data = await response.json()
    setUser(data.user)
  }

  async function logout() {
    await fetch(`${API_BASE_URL}/api/auth/logout`, {
      method: 'POST',
      credentials: 'include',
    })
    setUser(null)
  }

  async function setRequestMode(mode: RequestMode) {
    if (!user?.canChooseRequestMode) return

    const response = await fetch(`${API_BASE_URL}/api/users/me/preferences`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'include',
      body: JSON.stringify({ preferredRequestMode: mode }),
    })

    if (response.ok) {
      setUser({ ...user, requestMode: mode })
    }
  }

  return (
    <AuthContext.Provider
      value={{
        user,
        isLoading,
        isAdmin: user?.role === 'admin',
        requestMode: user?.requestMode ?? 'simple',
        canChooseRequestMode: user?.canChooseRequestMode ?? false,
        setRequestMode,
        login,
        logout,
      }}
    >
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const context = useContext(AuthContext)
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider')
  }
  return context
}
