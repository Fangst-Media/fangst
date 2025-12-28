import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { RouterProvider, createRouter, createRootRoute, createRoute, Outlet, Navigate } from '@tanstack/react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthProvider, useAuth } from './lib/auth'
import { ConnectionProvider } from './lib/connection'
import { Layout } from './components/Layout'
import Login from './routes/login'
import Dashboard from './routes/dashboard'
import Library from './routes/library'
import MediaDetails from './routes/media-details'
import Requests from './routes/requests'
import RequestsList from './routes/requests-list'
import Calendar from './routes/calendar'
import Activity from './routes/activity'
import Indexers from './routes/indexers'
import Downloaders from './routes/downloaders'
import Settings from './routes/settings'

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

  return <Navigate to="/discover" />
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
    return <Navigate to="/discover" />
  }

  return <Layout>{children}</Layout>
}

function LoginPage() {
  const { user, isAdmin } = useAuth()

  if (user) {
    return <Navigate to={isAdmin ? '/dashboard' : '/discover'} />
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

const mediaDetailsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/library/$slug',
  component: function MediaDetailsPage() {
    const { slug } = mediaDetailsRoute.useParams()
    return (
      <ProtectedRoute>
        <MediaDetails slug={slug} />
      </ProtectedRoute>
    )
  },
})

const requestsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/discover',
  component: () => (
    <ProtectedRoute>
      <Requests />
    </ProtectedRoute>
  ),
})

const requestsListRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/requests',
  component: () => (
    <ProtectedRoute>
      <RequestsList />
    </ProtectedRoute>
  ),
})

const calendarRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/calendar',
  component: () => (
    <ProtectedRoute>
      <Calendar />
    </ProtectedRoute>
  ),
})

const activityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activity',
  component: () => (
    <ProtectedRoute adminOnly>
      <Activity />
    </ProtectedRoute>
  ),
})

const indexersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/indexers',
  component: () => (
    <ProtectedRoute adminOnly>
      <Indexers />
    </ProtectedRoute>
  ),
})

const downloadersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/downloaders',
  component: () => (
    <ProtectedRoute adminOnly>
      <Downloaders />
    </ProtectedRoute>
  ),
})

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings',
  component: () => (
    <ProtectedRoute adminOnly>
      <Settings />
    </ProtectedRoute>
  ),
})

const routeTree = rootRoute.addChildren([indexRoute, loginRoute, dashboardRoute, libraryRoute, mediaDetailsRoute, requestsRoute, requestsListRoute, calendarRoute, activityRoute, indexersRoute, downloadersRoute, settingsRoute])

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
      <ConnectionProvider>
        <AuthProvider>
          <RouterProvider router={router} />
        </AuthProvider>
      </ConnectionProvider>
    </QueryClientProvider>
  </StrictMode>
)
