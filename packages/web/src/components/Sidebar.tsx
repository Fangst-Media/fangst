import { Link, useRouterState } from '@tanstack/react-router'
import { useAuth } from '../lib/auth'

type NavItem = {
  label: string
  href: string
  icon: React.ReactNode
  adminOnly?: boolean
}

const navItems: NavItem[] = [
  {
    label: 'Dashboard',
    href: '/dashboard',
    adminOnly: true,
    icon: (
      <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" />
      </svg>
    ),
  },
  {
    label: 'Library',
    href: '/library',
    icon: (
      <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z" />
      </svg>
    ),
  },
  {
    label: 'Discover',
    href: '/requests',
    icon: (
      <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>
    ),
  },
]

type SidebarProps = {
  collapsed: boolean
  onToggle: () => void
}

export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  const { user, logout, isAdmin } = useAuth()
  const routerState = useRouterState()
  const currentPath = routerState.location.pathname

  const filteredItems = navItems.filter((item) => !item.adminOnly || isAdmin)

  return (
    <aside
      className={`fixed left-0 top-0 z-40 h-screen bg-card border-r border-border transition-all duration-300 flex flex-col ${
        collapsed ? 'w-16' : 'w-56'
      }`}
    >
      {/* Logo */}
      <div className="h-16 flex items-center justify-between px-4 border-b border-border">
        {!collapsed && (
          <Link to="/" className="text-xl font-bold text-accent">
            Jæger
          </Link>
        )}
        <button
          type="button"
          onClick={onToggle}
          className={`p-2 rounded-md text-muted hover:text-foreground hover:bg-background transition-colors ${
            collapsed ? 'mx-auto' : ''
          }`}
          title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            {collapsed ? (
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M13 5l7 7-7 7M5 5l7 7-7 7" />
            ) : (
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M11 19l-7-7 7-7m8 14l-7-7 7-7" />
            )}
          </svg>
        </button>
      </div>

      {/* Navigation */}
      <nav className="flex-1 py-4 px-2 space-y-1 overflow-y-auto">
        {filteredItems.map((item) => {
          const isActive = currentPath === item.href
          return (
            <Link
              key={item.href + item.label}
              to={item.href}
              className={`flex items-center gap-3 px-3 py-2.5 rounded-md transition-colors ${
                isActive
                  ? 'bg-accent text-white'
                  : 'text-muted hover:text-foreground hover:bg-background'
              } ${collapsed ? 'justify-center' : ''}`}
              title={collapsed ? item.label : undefined}
            >
              {item.icon}
              {!collapsed && <span className="text-sm font-medium">{item.label}</span>}
            </Link>
          )
        })}
      </nav>

      {/* User section */}
      <div className="border-t border-border p-3">
        {collapsed ? (
          <button
            type="button"
            onClick={logout}
            className="w-full flex items-center justify-center p-2 rounded-md text-muted hover:text-foreground hover:bg-background transition-colors"
            title="Sign out"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
            </svg>
          </button>
        ) : (
          <div className="space-y-2">
            <div className="flex items-center gap-3 px-2">
              <div className="w-8 h-8 rounded-full bg-accent/20 flex items-center justify-center">
                <span className="text-sm font-medium text-accent">
                  {user?.name?.charAt(0).toUpperCase()}
                </span>
              </div>
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-foreground truncate">{user?.name}</p>
                <p className="text-xs text-muted capitalize">{user?.role}</p>
              </div>
            </div>
            <button
              type="button"
              onClick={logout}
              className="w-full flex items-center gap-3 px-3 py-2 rounded-md text-muted hover:text-foreground hover:bg-background transition-colors"
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
              </svg>
              <span className="text-sm">Sign out</span>
            </button>
          </div>
        )}
      </div>
    </aside>
  )
}
