import { TopNav } from './TopNav'

type LayoutProps = {
  children: React.ReactNode
}

export function Layout({ children }: LayoutProps) {
  return (
    <div className="min-h-screen bg-background">
      <TopNav />
      <main className="pt-16">{children}</main>
    </div>
  )
}
