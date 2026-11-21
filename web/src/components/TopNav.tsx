import { Link, useRouterState } from '@tanstack/react-router'
import { useState } from 'react'
import { useScrolled } from '../hooks/useScrolled'
import { UserDropdown } from './UserDropdown'

const navLinks = [
  { label: 'Home', href: '/' },
  { label: 'Movies', href: '/movies' },
  { label: 'TV Shows', href: '/tv' },
  { label: 'Books', href: '/books' },
]

export function TopNav() {
  const scrolled = useScrolled()
  const routerState = useRouterState()
  const currentPath = routerState.location.pathname
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false)

  const isOpaque = scrolled || mobileMenuOpen

  return (
    <header
      className={`fixed top-0 left-0 right-0 z-50 transition-all duration-300 ${
        isOpaque
          ? 'bg-background/95 backdrop-blur-md border-b border-border'
          : 'bg-gradient-to-b from-black/80 to-transparent'
      }`}
    >
      <nav className="flex items-center justify-between h-16 px-6 max-w-[1920px] mx-auto">
        {/* Left: Logo + Links */}
        <div className="flex items-center gap-8">
          <Link to="/" className="text-xl font-bold text-accent shrink-0">
            Jæger
          </Link>

          {/* Desktop nav links */}
          <div className="hidden md:flex items-center gap-1">
            {navLinks.map((link) => {
              const isActive =
                currentPath === link.href ||
                (link.href !== '/' && currentPath.startsWith(link.href))
              return (
                <Link
                  key={link.href}
                  to={link.href}
                  className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${
                    isActive ? 'text-foreground' : 'text-foreground/60 hover:text-foreground'
                  }`}
                >
                  {link.label}
                </Link>
              )
            })}
          </div>
        </div>

        {/* Right: Search + User */}
        <div className="flex items-center gap-4">
          <Link
            to="/search"
            className="p-2 text-foreground/60 hover:text-foreground transition-colors"
          >
            <svg
              aria-hidden="true"
              className="w-5 h-5"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={1.5}
                d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
              />
            </svg>
          </Link>

          <UserDropdown />

          {/* Mobile hamburger */}
          <button
            type="button"
            onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
            className="md:hidden p-2 text-foreground/60 hover:text-foreground transition-colors"
          >
            <svg
              aria-hidden="true"
              className="w-5 h-5"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              {mobileMenuOpen ? (
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M6 18L18 6M6 6l12 12"
                />
              ) : (
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M4 6h16M4 12h16M4 18h16"
                />
              )}
            </svg>
          </button>
        </div>
      </nav>

      {/* Mobile menu */}
      {mobileMenuOpen && (
        <div className="md:hidden border-t border-border bg-background/95 backdrop-blur-md">
          <div className="px-4 py-3 space-y-1">
            {navLinks.map((link) => {
              const isActive =
                currentPath === link.href ||
                (link.href !== '/' && currentPath.startsWith(link.href))
              return (
                <Link
                  key={link.href}
                  to={link.href}
                  onClick={() => setMobileMenuOpen(false)}
                  className={`block px-3 py-2 rounded-md text-sm font-medium transition-colors ${
                    isActive
                      ? 'text-foreground bg-card'
                      : 'text-foreground/60 hover:text-foreground hover:bg-card'
                  }`}
                >
                  {link.label}
                </Link>
              )
            })}
          </div>
        </div>
      )}
    </header>
  )
}
