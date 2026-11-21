type QualityBadgesProps = {
  hdr?: 'dv' | 'hdr10+' | 'hdr10' | 'hlg'
  audioCodec?: string
  streamingService?: string
  isRemux?: boolean
  proper?: boolean
  repack?: boolean
}

const hdrConfig: Record<string, { bg: string; label: string }> = {
  dv: { bg: 'bg-purple-500', label: 'DV' },
  'hdr10+': { bg: 'bg-amber-500', label: 'HDR10+' },
  hdr10: { bg: 'bg-orange-500', label: 'HDR10' },
  hlg: { bg: 'bg-teal-500', label: 'HLG' },
}

const audioHighlight = ['atmos', 'truehd', 'dtsx']

const serviceColors: Record<string, string> = {
  NF: 'bg-red-600',
  AMAZ: 'bg-sky-600',
  DSNP: 'bg-blue-600',
  ATVP: 'bg-gray-600',
  HULU: 'bg-green-600',
  HMAX: 'bg-purple-600',
  PCOK: 'bg-yellow-600',
  PMTP: 'bg-blue-700',
}

export function QualityBadges({
  hdr,
  audioCodec,
  streamingService,
  isRemux,
  proper,
  repack,
}: QualityBadgesProps) {
  const badges: { label: string; bg: string }[] = []

  if (hdr && hdrConfig[hdr]) {
    badges.push(hdrConfig[hdr])
  }

  if (audioCodec && audioHighlight.includes(audioCodec)) {
    badges.push({
      bg: 'bg-cyan-600',
      label: audioCodec.toUpperCase(),
    })
  }

  if (streamingService) {
    badges.push({
      bg: serviceColors[streamingService] || 'bg-gray-600',
      label: streamingService,
    })
  }

  if (isRemux) {
    badges.push({ bg: 'bg-indigo-600', label: 'Remux' })
  }

  if (proper) {
    badges.push({ bg: 'bg-emerald-600', label: 'Proper' })
  }

  if (repack) {
    badges.push({ bg: 'bg-emerald-700', label: 'Repack' })
  }

  if (badges.length === 0) return null

  return (
    <div className="flex flex-wrap gap-1">
      {badges.map((badge) => (
        <span
          key={badge.label}
          className={`${badge.bg} px-1.5 py-0.5 rounded text-[10px] font-medium text-white uppercase`}
        >
          {badge.label}
        </span>
      ))}
    </div>
  )
}
