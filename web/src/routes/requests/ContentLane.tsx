import type { LaneResponse, MetadataResult } from '@jaeger/shared/schemas/metadata'
import { TrendingLane } from './TrendingLane'

export type { LaneResponse }

function UnsupportedLane({ title }: { title: string }) {
  return (
    <div className="mb-8 p-4 rounded-lg bg-card border border-border">
      <h2 className="text-lg font-semibold text-foreground mb-2">{title}</h2>
      <p className="text-sm text-muted">
        No metadata provider supports this capability. Configure a TMDB API key in settings.
      </p>
    </div>
  )
}

type ContentLaneProps = {
  title: string
  data: LaneResponse | undefined
  isAdmin: boolean
  onSelect?: (item: MetadataResult) => void
}

export function ContentLane({ title, data, isAdmin, onSelect }: ContentLaneProps) {
  if (!data) return null

  if (data.unsupported) {
    if (!isAdmin) return null
    return <UnsupportedLane title={title} />
  }

  if (data.results.length === 0) return null

  return <TrendingLane title={title} items={data.results} onSelect={onSelect} />
}
