import { Link } from '@tanstack/react-router'

export default function Index() {
  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="text-center">
        <h1 className="text-6xl font-bold text-accent mb-4">Fangst</h1>
        <p className="text-xl text-muted mb-8">Unified Media Management</p>

        <Link
          to="/library"
          className="inline-block glass rounded-lg px-8 py-4 font-semibold hover:bg-accent/10 transition-colors"
        >
          Browse Library
        </Link>

        <div className="mt-8 glass rounded-lg p-6 max-w-md mx-auto">
          <p className="text-sm text-muted">
            Your files, your way. No forced renaming, portable metadata.
          </p>
        </div>
      </div>
    </div>
  )
}
