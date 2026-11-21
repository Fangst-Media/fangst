import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
  Navigate,
  Outlet,
  RouterProvider,
  createRootRoute,
  createRoute,
  createRouter,
} from '@tanstack/react-router'
import { AdminLayout } from './components/AdminLayout'
import { Layout } from './components/Layout'
import { DownloadsLayout } from './components/downloads/DownloadsLayout'
import { AuthProvider, useAuth } from './lib/auth'
import { ConnectionProvider } from './lib/connection'
import Activity from './routes/activity'
import BookDetail from './routes/book-detail'
import Books from './routes/books'
import Calendar from './routes/calendar'
import CollectionPage from './routes/collection'
import Dashboard from './routes/dashboard'
import Downloaders from './routes/downloaders'
import JobDetail from './routes/downloads/job-detail'
import Queue from './routes/downloads/queue'
import Home from './routes/home'
import Indexers from './routes/indexers'
import Library from './routes/library'
import Login from './routes/login'
import MediaDetails from './routes/media-details'
import MovieDetail from './routes/movie-detail'
import Movies from './routes/movies'
import RequestsList from './routes/requests-list'
import Search from './routes/search'
import SeasonPage from './routes/season'
import Settings from './routes/settings'
import TvSeriesPage from './routes/tv-series'
import TvShows from './routes/tv-shows'

const queryClient = new QueryClient()

const rootRoute = createRootRoute({
  component: () => <Outlet />,
})

// The old TypeScript API's health check and login only gate the old pages; the download
// daemon pages talk to a different service and must work without it.
const legacyRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'legacy',
  component: () => (
    <ConnectionProvider>
      <AuthProvider>
        <Outlet />
      </AuthProvider>
    </ConnectionProvider>
  ),
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

function ProtectedRoute({
  children,
  adminOnly = false,
}: { children: React.ReactNode; adminOnly?: boolean }) {
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
  getParentRoute: () => legacyRoute,
  path: '/',
  component: IndexRedirect,
})

const loginRoute = createRoute({
  getParentRoute: () => legacyRoute,
  path: '/login',
  component: LoginPage,
})

const moviesRoute = createRoute({
  getParentRoute: () => legacyRoute,
  path: '/movies',
  component: () => (
    <ProtectedRoute>
      <Movies />
    </ProtectedRoute>
  ),
})

const tvShowsRoute = createRoute({
  getParentRoute: () => legacyRoute,
  path: '/tv',
  component: () => (
    <ProtectedRoute>
      <TvShows />
    </ProtectedRoute>
  ),
})

const movieDetailRoute = createRoute({
  getParentRoute: () => legacyRoute,
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
  getParentRoute: () => legacyRoute,
  path: '/books',
  component: () => (
    <ProtectedRoute>
      <Books />
    </ProtectedRoute>
  ),
})

const bookDetailRoute = createRoute({
  getParentRoute: () => legacyRoute,
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
  getParentRoute: () => legacyRoute,
  path: '/search',
  component: () => (
    <ProtectedRoute>
      <Search />
    </ProtectedRoute>
  ),
})

// --- User routes ---

const myRequestsRoute = createRoute({
  getParentRoute: () => legacyRoute,
  path: '/my-requests',
  component: () => (
    <ProtectedRoute>
      <RequestsList />
    </ProtectedRoute>
  ),
})

const calendarRoute = createRoute({
  getParentRoute: () => legacyRoute,
  path: '/calendar',
  component: () => (
    <ProtectedRoute>
      <Calendar />
    </ProtectedRoute>
  ),
})

// --- Library routes ---

const libraryRoute = createRoute({
  getParentRoute: () => legacyRoute,
  path: '/library',
  component: () => (
    <ProtectedRoute>
      <Library />
    </ProtectedRoute>
  ),
})

const mediaDetailsRoute = createRoute({
  getParentRoute: () => legacyRoute,
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
  getParentRoute: () => legacyRoute,
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
  getParentRoute: () => legacyRoute,
  path: '/library/tv/$tmdbId/season/$seasonNumber',
  component: function SeasonRoutePage() {
    const { tmdbId, seasonNumber } = seasonRoute.useParams()
    return (
      <ProtectedRoute>
        <SeasonPage tmdbId={tmdbId} seasonNumber={Number.parseInt(seasonNumber, 10)} />
      </ProtectedRoute>
    )
  },
})

const collectionRoute = createRoute({
  getParentRoute: () => legacyRoute,
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
  getParentRoute: () => legacyRoute,
  path: '/admin',
  component: () => (
    <AdminRoute>
      <Dashboard />
    </AdminRoute>
  ),
})

const adminRequestsRoute = createRoute({
  getParentRoute: () => legacyRoute,
  path: '/admin/requests',
  component: () => (
    <AdminRoute>
      <RequestsList />
    </AdminRoute>
  ),
})

const adminActivityRoute = createRoute({
  getParentRoute: () => legacyRoute,
  path: '/admin/activity',
  component: () => (
    <AdminRoute>
      <Activity />
    </AdminRoute>
  ),
})

const adminIndexersRoute = createRoute({
  getParentRoute: () => legacyRoute,
  path: '/admin/indexers',
  component: () => (
    <AdminRoute>
      <Indexers />
    </AdminRoute>
  ),
})

const adminDownloadersRoute = createRoute({
  getParentRoute: () => legacyRoute,
  path: '/admin/downloaders',
  component: () => (
    <AdminRoute>
      <Downloaders />
    </AdminRoute>
  ),
})

const adminSettingsRoute = createRoute({
  getParentRoute: () => legacyRoute,
  path: '/admin/settings',
  component: () => (
    <AdminRoute>
      <Settings />
    </AdminRoute>
  ),
})

// --- Download daemon routes ---

const downloadsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/downloads',
  component: () => (
    <DownloadsLayout>
      <Queue />
    </DownloadsLayout>
  ),
})

const jobDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/downloads/$jobId',
  component: function JobDetailPage() {
    const { jobId } = jobDetailRoute.useParams()
    const id = Number(jobId)
    return (
      <DownloadsLayout>
        {Number.isSafeInteger(id) && id > 0 ? (
          <JobDetail jobId={id} />
        ) : (
          <p className="text-muted">"{jobId}" is not a job ID.</p>
        )}
      </DownloadsLayout>
    )
  },
})

// --- Router ---

const legacyTree = legacyRoute.addChildren([
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

const routeTree = rootRoute.addChildren([legacyTree, downloadsRoute, jobDetailRoute])

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
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>
)
