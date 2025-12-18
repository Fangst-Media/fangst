import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { RouterProvider, createRouter, createRootRoute, createRoute, Outlet, Navigate } from '@tanstack/react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthProvider, useAuth } from './lib/auth'
import { Layout } from './components/Layout'
import Login from './routes/login'
import Dashboard from './routes/dashboard'
import Library from './routes/library'
import Requests from './routes/requests'

const queryClient = new QueryClient()

const rootRoute = createRootRoute({
  component: () => <Outlet />,
})

function IndexRedirect() {
  const { user, isLoading, isAdmin } = useAuth()

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
      </div>
    )
  }

  if (!user) {
    return <Navigate to="/login" />
  }

  if (isAdmin) {
    return <Navigate to="/dashboard" />
  }

  return <Navigate to="/library" />
}

function ProtectedRoute({ children, adminOnly = false }: { children: React.ReactNode; adminOnly?: boolean }) {
  const { user, isLoading, isAdmin } = useAuth()

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
      </div>
    )
  }

  if (!user) {
    return <Navigate to="/login" />
  }

  if (adminOnly && !isAdmin) {
    return <Navigate to="/library" />
  }

  return <Layout>{children}</Layout>
}

function LoginPage() {
  const { user, isAdmin } = useAuth()

  if (user) {
    return <Navigate to={isAdmin ? '/dashboard' : '/library'} />
  }

  return <Login onSuccess={() => window.location.reload()} />
}

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: IndexRedirect,
})

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/login',
  component: LoginPage,
})

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/dashboard',
  component: () => (
    <ProtectedRoute adminOnly>
      <Dashboard />
    </ProtectedRoute>
  ),
})

const libraryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/library',
  component: () => (
    <ProtectedRoute>
      <Library />
    </ProtectedRoute>
  ),
})

const requestsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/requests',
  component: () => (
    <ProtectedRoute>
      <Requests />
    </ProtectedRoute>
  ),
})

const routeTree = rootRoute.addChildren([indexRoute, loginRoute, dashboardRoute, libraryRoute, requestsRoute])

const router = createRouter({
  routeTree,
  defaultPreload: 'intent',
})

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <AuthProvider>
        <RouterProvider router={router} />
      </AuthProvider>
    </QueryClientProvider>
  </StrictMode>
)
