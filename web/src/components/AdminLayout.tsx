import { Link, useRouterState } from '@tanstack/react-router'
import { TopNav } from './TopNav'

const adminTabs = [
  { label: 'Dashboard', href: '/admin' },
  { label: 'Requests', href: '/admin/requests' },
  { label: 'Activity', href: '/admin/activity' },
  { label: 'Indexers', href: '/admin/indexers' },
  { label: 'Downloaders', href: '/admin/downloaders' },
  { label: 'Settings', href: '/admin/settings' },
]

export function AdminLayout({ children }: { children: React.ReactNode }) {
  const routerState = useRouterState()
  const currentPath = routerState.location.pathname

  return (
    <div className="min-h-screen bg-background">
      <TopNav />
      <div className="pt-16">
        <div className="border-b border-border bg-card/50">
          <div className="max-w-7xl mx-auto px-6">
            <div className="flex items-center gap-1 overflow-x-auto scrollbar-hide">
              {adminTabs.map((tab) => {
                const isActive = currentPath === tab.href
                return (
                  <Link
                    key={tab.href}
                    to={tab.href}
                    className={`px-4 py-3 text-sm font-medium border-b-2 transition-colors whitespace-nowrap ${
                      isActive
                        ? 'border-accent text-foreground'
                        : 'border-transparent text-muted hover:text-foreground'
                    }`}
                  >
                    {tab.label}
                  </Link>
                )
              })}
            </div>
          </div>
        </div>
        <main>{children}</main>
      </div>
    </div>
  )
}
