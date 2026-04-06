import { useState, useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { apiFetch } from '../lib/api-client'

type CalendarItem = {
  date: string
  type: 'episode' | 'movie'
  title: string
  episode?: {
    season: number
    episode: number
    title: string | null
  }
  status: string
  requestId: string
  posterPath: string | null
}

type ViewMode = 'week' | 'month'

async function fetchCalendar(start: string, end: string): Promise<{ items: CalendarItem[] }> {
  return apiFetch(`/api/calendar?start=${start}&end=${end}`)
}

function getPosterUrl(path: string | null, size = 'w92'): string {
  if (!path) return '/placeholder-poster.png'
  return `https://image.tmdb.org/t/p/${size}${path}`
}

function formatDate(dateStr: string): string {
  const date = new Date(dateStr + 'T00:00:00')
  return date.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' })
}

function getWeekDates(baseDate: Date): { start: string; end: string; dates: Date[] } {
  const dates: Date[] = []
  const startOfWeek = new Date(baseDate)
  startOfWeek.setDate(baseDate.getDate() - baseDate.getDay())

  for (let i = 0; i < 7; i++) {
    const d = new Date(startOfWeek)
    d.setDate(startOfWeek.getDate() + i)
    dates.push(d)
  }

  const start = dates[0]!.toISOString().split('T')[0]!
  const end = dates[6]!.toISOString().split('T')[0]!

  return { start, end, dates }
}

function getMonthDates(baseDate: Date): { start: string; end: string; dates: Date[] } {
  const year = baseDate.getFullYear()
  const month = baseDate.getMonth()

  const firstDay = new Date(year, month, 1)
  const lastDay = new Date(year, month + 1, 0)

  const dates: Date[] = []

  // Start from Sunday of the first week
  const startDate = new Date(firstDay)
  startDate.setDate(firstDay.getDate() - firstDay.getDay())

  // End on Saturday of the last week
  const endDate = new Date(lastDay)
  endDate.setDate(lastDay.getDate() + (6 - lastDay.getDay()))

  const current = new Date(startDate)
  while (current <= endDate) {
    dates.push(new Date(current))
    current.setDate(current.getDate() + 1)
  }

  const start = startDate.toISOString().split('T')[0]!
  const end = endDate.toISOString().split('T')[0]!

  return { start, end, dates }
}

function StatusDot({ status }: { status: string }) {
  const colors: Record<string, string> = {
    pending: 'bg-yellow-500',
    searching: 'bg-blue-500',
    downloading: 'bg-blue-400',
    completed: 'bg-green-500',
    failed: 'bg-red-500',
  }
  return <span className={`w-2 h-2 rounded-full ${colors[status] ?? 'bg-gray-500'}`} />
}

function CalendarItemCard({ item }: { item: CalendarItem }) {
  const episodeCode = item.episode
    ? `S${String(item.episode.season).padStart(2, '0')}E${String(item.episode.episode).padStart(2, '0')}`
    : null

  return (
    <Link
      to={`/my-requests`}
      className="flex items-center gap-2 p-2 rounded-lg bg-background hover:bg-card border border-border transition-colors group"
    >
      <img
        src={getPosterUrl(item.posterPath)}
        alt={item.title}
        className="w-8 h-12 object-cover rounded hidden sm:block"
      />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <StatusDot status={item.status} />
          <span className="text-sm font-medium text-foreground truncate">{item.title}</span>
        </div>
        {episodeCode && (
          <p className="text-xs text-muted truncate">
            {episodeCode}{item.episode?.title ? ` - ${item.episode.title}` : ''}
          </p>
        )}
        {item.type === 'movie' && (
          <p className="text-xs text-muted">Movie</p>
        )}
      </div>
    </Link>
  )
}

function WeekView({ dates, items }: { dates: Date[]; items: CalendarItem[] }) {
  const today = new Date().toISOString().split('T')[0]!

  const itemsByDate = useMemo(() => {
    const map = new Map<string, CalendarItem[]>()
    for (const item of items) {
      const list = map.get(item.date) ?? []
      list.push(item)
      map.set(item.date, list)
    }
    return map
  }, [items])

  return (
    <div className="grid grid-cols-7 gap-2">
      {dates.map((date) => {
        const dateStr = date.toISOString().split('T')[0]!
        const dayItems = itemsByDate.get(dateStr) ?? []
        const isToday = dateStr === today
        const isPast = dateStr < today

        return (
          <div
            key={dateStr}
            className={`min-h-[200px] rounded-lg border p-2 ${
              isToday
                ? 'border-accent bg-accent/5'
                : isPast
                  ? 'border-border/50 bg-background/50'
                  : 'border-border bg-card'
            }`}
          >
            <div className={`text-sm font-medium mb-2 ${isToday ? 'text-accent' : isPast ? 'text-muted' : 'text-foreground'}`}>
              {date.toLocaleDateString('en-US', { weekday: 'short' })}
              <span className="ml-1 text-lg">{date.getDate()}</span>
            </div>
            <div className="space-y-1">
              {dayItems.map((item, i) => (
                <CalendarItemCard key={`${item.requestId}-${i}`} item={item} />
              ))}
              {dayItems.length === 0 && (
                <p className="text-xs text-muted/50 text-center py-4">No items</p>
              )}
            </div>
          </div>
        )
      })}
    </div>
  )
}

function MonthView({ dates, items, baseDate }: { dates: Date[]; items: CalendarItem[]; baseDate: Date }) {
  const today = new Date().toISOString().split('T')[0]!
  const currentMonth = baseDate.getMonth()

  const itemsByDate = useMemo(() => {
    const map = new Map<string, CalendarItem[]>()
    for (const item of items) {
      const list = map.get(item.date) ?? []
      list.push(item)
      map.set(item.date, list)
    }
    return map
  }, [items])

  const weeks: Date[][] = []
  for (let i = 0; i < dates.length; i += 7) {
    weeks.push(dates.slice(i, i + 7))
  }

  return (
    <div className="space-y-1">
      {/* Header */}
      <div className="grid grid-cols-7 gap-1">
        {['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'].map((day) => (
          <div key={day} className="text-center text-xs font-medium text-muted py-2">
            {day}
          </div>
        ))}
      </div>

      {/* Weeks */}
      {weeks.map((week, weekIdx) => (
        <div key={weekIdx} className="grid grid-cols-7 gap-1">
          {week.map((date) => {
            const dateStr = date.toISOString().split('T')[0]!
            const dayItems = itemsByDate.get(dateStr) ?? []
            const isToday = dateStr === today
            const isCurrentMonth = date.getMonth() === currentMonth
            const isPast = dateStr < today

            return (
              <div
                key={dateStr}
                className={`min-h-[100px] rounded-lg border p-1 ${
                  isToday
                    ? 'border-accent bg-accent/5'
                    : !isCurrentMonth
                      ? 'border-border/30 bg-background/30'
                      : isPast
                        ? 'border-border/50 bg-background/50'
                        : 'border-border bg-card'
                }`}
              >
                <div className={`text-xs font-medium mb-1 ${
                  isToday ? 'text-accent' : !isCurrentMonth ? 'text-muted/50' : isPast ? 'text-muted' : 'text-foreground'
                }`}>
                  {date.getDate()}
                </div>
                <div className="space-y-0.5">
                  {dayItems.slice(0, 3).map((item, i) => (
                    <div
                      key={`${item.requestId}-${i}`}
                      className="flex items-center gap-1 text-xs truncate p-0.5 rounded bg-background"
                      title={`${item.title}${item.episode ? ` - S${item.episode.season}E${item.episode.episode}` : ''}`}
                    >
                      <StatusDot status={item.status} />
                      <span className="truncate">{item.title}</span>
                    </div>
                  ))}
                  {dayItems.length > 3 && (
                    <div className="text-xs text-muted text-center">+{dayItems.length - 3} more</div>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      ))}
    </div>
  )
}

export default function Calendar() {
  const [viewMode, setViewMode] = useState<ViewMode>('week')
  const [baseDate, setBaseDate] = useState(new Date())

  const { start, end, dates } = viewMode === 'week'
    ? getWeekDates(baseDate)
    : getMonthDates(baseDate)

  const { data, isLoading } = useQuery({
    queryKey: ['calendar', start, end],
    queryFn: () => fetchCalendar(start, end),
  })

  const items = data?.items ?? []

  const navigate = (direction: 'prev' | 'next') => {
    const newDate = new Date(baseDate)
    if (viewMode === 'week') {
      newDate.setDate(baseDate.getDate() + (direction === 'next' ? 7 : -7))
    } else {
      newDate.setMonth(baseDate.getMonth() + (direction === 'next' ? 1 : -1))
    }
    setBaseDate(newDate)
  }

  const goToToday = () => {
    setBaseDate(new Date())
  }

  const headerText = viewMode === 'week'
    ? `${formatDate(start)} - ${formatDate(end)}`
    : baseDate.toLocaleDateString('en-US', { month: 'long', year: 'numeric' })

  return (
    <div className="p-6">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold text-foreground">Calendar</h1>

        <div className="flex items-center gap-2">
          {/* View toggle */}
          <div className="flex rounded-lg border border-border overflow-hidden">
            <button
              type="button"
              onClick={() => setViewMode('week')}
              className={`px-3 py-1.5 text-sm font-medium transition-colors ${
                viewMode === 'week'
                  ? 'bg-accent text-white'
                  : 'bg-card text-muted hover:text-foreground'
              }`}
            >
              Week
            </button>
            <button
              type="button"
              onClick={() => setViewMode('month')}
              className={`px-3 py-1.5 text-sm font-medium transition-colors ${
                viewMode === 'month'
                  ? 'bg-accent text-white'
                  : 'bg-card text-muted hover:text-foreground'
              }`}
            >
              Month
            </button>
          </div>
        </div>
      </div>

      {/* Navigation */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => navigate('prev')}
            className="p-2 rounded-lg bg-card border border-border hover:bg-background transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
            </svg>
          </button>
          <button
            type="button"
            onClick={goToToday}
            className="px-3 py-1.5 text-sm font-medium rounded-lg bg-card border border-border hover:bg-background transition-colors"
          >
            Today
          </button>
          <button
            type="button"
            onClick={() => navigate('next')}
            className="p-2 rounded-lg bg-card border border-border hover:bg-background transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
            </svg>
          </button>
        </div>

        <h2 className="text-lg font-semibold text-foreground">{headerText}</h2>

        {/* Legend */}
        <div className="flex items-center gap-4 text-xs text-muted">
          <div className="flex items-center gap-1">
            <span className="w-2 h-2 rounded-full bg-yellow-500" />
            <span>Pending</span>
          </div>
          <div className="flex items-center gap-1">
            <span className="w-2 h-2 rounded-full bg-blue-500" />
            <span>Downloading</span>
          </div>
          <div className="flex items-center gap-1">
            <span className="w-2 h-2 rounded-full bg-green-500" />
            <span>Completed</span>
          </div>
        </div>
      </div>

      {/* Calendar */}
      {isLoading ? (
        <div className="flex items-center justify-center h-64">
          <div className="h-10 w-10 animate-spin rounded-full border-2 border-accent border-t-transparent" />
        </div>
      ) : viewMode === 'week' ? (
        <WeekView dates={dates} items={items} />
      ) : (
        <MonthView dates={dates} items={items} baseDate={baseDate} />
      )}

      {/* Empty state */}
      {!isLoading && items.length === 0 && (
        <div className="text-center py-12">
          <svg className="w-16 h-16 mx-auto text-muted/30 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          <p className="text-muted">No upcoming episodes or movies in this period</p>
        </div>
      )}
    </div>
  )
}
