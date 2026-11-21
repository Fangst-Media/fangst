import { QualityScore } from './QualityScore'
import { TierBadge } from './TierBadge'
import { QualityBadges } from './QualityBadges'
import type { ParsedReleaseInfo } from '@jaeger/shared/types/indexer'

type QualitySummaryProps = {
  parsed: ParsedReleaseInfo
  showExpandButton?: boolean
  expanded?: boolean
  onToggleExpand?: () => void
}

export function QualitySummary({
  parsed,
  showExpandButton = false,
  expanded = false,
  onToggleExpand,
}: QualitySummaryProps) {
  return (
    <div className="flex items-center gap-2 flex-wrap">
      {parsed.qualityScore !== undefined && <QualityScore score={parsed.qualityScore} />}

      {parsed.resolution && (
        <span className="px-1.5 py-0.5 rounded bg-black/50 text-[10px] font-medium text-white uppercase">
          {parsed.resolution}
        </span>
      )}

      <TierBadge tier={parsed.releaseGroupTier} />

      <QualityBadges
        hdr={parsed.hdr}
        audioCodec={parsed.audioCodec}
        streamingService={parsed.streamingService}
        isRemux={parsed.isRemux}
        proper={parsed.proper}
        repack={parsed.repack}
      />

      {showExpandButton && onToggleExpand && (
        <button
          type="button"
          onClick={onToggleExpand}
          className="p-1 rounded hover:bg-white/10 transition-colors text-muted hover:text-foreground"
          title={expanded ? 'Hide details' : 'Show details'}
        >
          <svg
            className={`w-4 h-4 transition-transform ${expanded ? 'rotate-180' : ''}`}
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        </button>
      )}
    </div>
  )
}
