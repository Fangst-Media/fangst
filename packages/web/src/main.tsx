import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { RouterProvider, createRouter, createRootRoute, createRoute, Outlet, Navigate } from '@tanstack/react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthProvider, useAuth } from './lib/auth'
import { ConnectionProvider } from './lib/connection'
import { Layout } from './components/Layout'
import { AdminLayout } from './components/AdminLayout'
import Login from './routes/login'
import Home from './routes/home'
import Movies from './routes/movies'
import TvShows from './routes/tv-shows'
import Books from './routes/books'
import BookDetail from './routes/book-detail'
import Search from './routes/search'
import Library from './routes/library'
import MediaDetails from './routes/media-details'
import TvSeriesPage from './routes/tv-series'
import SeasonPage from './routes/season'
import CollectionPage from './routes/collection'
import RequestsList from './routes/requests-list'
import Calendar from './routes/calendar'
import Dashboard from './routes/dashboard'
import Activity from './routes/activity'
import Indexers from './routes/indexers'
import Downloaders from './routes/downloaders'
import Settings from './routes/settings'
import MovieDetail from './routes/movie-detail'

const queryClient = new QueryClient()

const rootRoute = createRootRoute({
  component: () => <Outlet />,
})

function IndexRedirect() {
  const { user, isLoading } = useAuth()

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

  return (
    <Layout>
      <Home />
    </Layout>
  )
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
    return <Navigate to="/" />
  }

  return <Layout>{children}</Layout>
}

function AdminRoute({ children }: { children: React.ReactNode }) {
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

  if (!isAdmin) {
    return <Navigate to="/" />
  }

  return <AdminLayout>{children}</AdminLayout>
}

function LoginPage() {
  const { user } = useAuth()

  if (user) {
    return <Navigate to="/" />
  }

  return <Login onSuccess={() => window.location.reload()} />
}

// --- Core routes ---

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

const moviesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/movies',
  component: () => (
    <ProtectedRoute>
      <Movies />
    </ProtectedRoute>
  ),
})

const tvShowsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/tv',
  component: () => (
    <ProtectedRoute>
      <TvShows />
    </ProtectedRoute>
  ),
})

const movieDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/movie/$tmdbId',
  component: function MovieDetailPage() {
    const { tmdbId } = movieDetailRoute.useParams()
    return (
      <ProtectedRoute>
        <MovieDetail tmdbId={tmdbId} />
      </ProtectedRoute>
    )
  },
})

const booksRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/books',
  component: () => (
    <ProtectedRoute>
      <Books />
    </ProtectedRoute>
  ),
})

const bookDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/book/$openLibraryId',
  component: function BookDetailPage() {
    const { openLibraryId } = bookDetailRoute.useParams()
    return (
      <ProtectedRoute>
        <BookDetail openLibraryId={openLibraryId} />
      </ProtectedRoute>
    )
  },
})

const searchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/search',
  component: () => (
    <ProtectedRoute>
      <Search />
    </ProtectedRoute>
  ),
})

// --- User routes ---

const myRequestsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/my-requests',
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

// --- Library routes ---

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

const tvSeriesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/library/tv/$tmdbId',
  component: function TvSeriesRoutePage() {
    const { tmdbId } = tvSeriesRoute.useParams()
    return (
      <ProtectedRoute>
        <TvSeriesPage tmdbId={tmdbId} />
      </ProtectedRoute>
    )
  },
})

const seasonRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/library/tv/$tmdbId/season/$seasonNumber',
  component: function SeasonRoutePage() {
    const { tmdbId, seasonNumber } = seasonRoute.useParams()
    return (
      <ProtectedRoute>
        <SeasonPage tmdbId={tmdbId} seasonNumber={parseInt(seasonNumber, 10)} />
      </ProtectedRoute>
    )
  },
})

const collectionRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/library/collection/$collectionId',
  component: function CollectionRoutePage() {
    const { collectionId } = collectionRoute.useParams()
    return (
      <ProtectedRoute>
        <CollectionPage collectionId={collectionId} />
      </ProtectedRoute>
    )
  },
})

// --- Admin routes ---

const adminDashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/admin',
  component: () => (
    <AdminRoute>
      <Dashboard />
    </AdminRoute>
  ),
})

const adminRequestsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/admin/requests',
  component: () => (
    <AdminRoute>
      <RequestsList />
    </AdminRoute>
  ),
})

const adminActivityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/admin/activity',
  component: () => (
    <AdminRoute>
      <Activity />
    </AdminRoute>
  ),
})

const adminIndexersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/admin/indexers',
  component: () => (
    <AdminRoute>
      <Indexers />
    </AdminRoute>
  ),
})

const adminDownloadersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/admin/downloaders',
  component: () => (
    <AdminRoute>
      <Downloaders />
    </AdminRoute>
  ),
})

const adminSettingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/admin/settings',
  component: () => (
    <AdminRoute>
      <Settings />
    </AdminRoute>
  ),
})

// --- Router ---

const routeTree = rootRoute.addChildren([
  indexRoute,
  loginRoute,
  moviesRoute,
  movieDetailRoute,
  tvShowsRoute,
  booksRoute,
  bookDetailRoute,
  searchRoute,
  myRequestsRoute,
  calendarRoute,
  libraryRoute,
  tvSeriesRoute,
  seasonRoute,
  collectionRoute,
  mediaDetailsRoute,
  adminDashboardRoute,
  adminRequestsRoute,
  adminActivityRoute,
  adminIndexersRoute,
  adminDownloadersRoute,
  adminSettingsRoute,
])

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
