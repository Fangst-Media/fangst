import { createRoute } from '@tanstack/react-router'
import { Route as RootRoute } from './__root'

export const Route = createRoute({
  getParentRoute: () => RootRoute,
  path: '/',
  component: Index,
})

function Index() {
  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="text-center">
        <h1 className="text-6xl font-bold text-accent mb-4">Jæger</h1>
        <p className="text-xl text-muted">Unified Media Management</p>
        <div className="mt-8 glass rounded-lg p-6 max-w-md">
          <p className="text-sm">
            Welcome to Jæger. Your files, your way.
          </p>
        </div>
      </div>
    </div>
  )
}
