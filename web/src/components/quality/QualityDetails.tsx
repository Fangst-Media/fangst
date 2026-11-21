import type { ParsedReleaseInfo } from '@jaeger/shared/types/indexer'

type QualityDetailsProps = {
  parsed: ParsedReleaseInfo
}

const sourceLabels: Record<string, string> = {
  bluray: 'Blu-ray',
  webdl: 'WEB-DL',
  webrip: 'WEBRip',
  hdtv: 'HDTV',
  dvd: 'DVD',
}

const codecLabels: Record<string, string> = {
  x265: 'x265/HEVC',
  x264: 'x264/AVC',
  av1: 'AV1',
  xvid: 'XviD',
}

const audioLabels: Record<string, string> = {
  atmos: 'Dolby Atmos',
  truehd: 'TrueHD',
  dtsx: 'DTS:X',
  dtshd: 'DTS-HD MA',
  dts: 'DTS',
  ddp: 'DD+',
  dd: 'DD 5.1',
  flac: 'FLAC',
  aac: 'AAC',
  ac3: 'AC3',
  opus: 'Opus',
}

const hdrLabels: Record<string, string> = {
  dv: 'Dolby Vision',
  'hdr10+': 'HDR10+',
  hdr10: 'HDR10',
  hlg: 'HLG',
}

const tierLabels: Record<number, string> = {
  1: 'Tier 1 (Golden)',
  2: 'Tier 2 (Silver)',
  3: 'Tier 3 (Bronze)',
}

function DetailRow({ label, value }: { label: string; value: string | undefined | null }) {
  if (!value) return null
  return (
    <div className="flex justify-between text-sm">
      <span className="text-muted">{label}</span>
      <span className="text-foreground font-medium">{value}</span>
    </div>
  )
}

export function QualityDetails({ parsed }: QualityDetailsProps) {
  const flags: string[] = []
  if (parsed.isRemux) flags.push('Remux')
  if (parsed.proper) flags.push('Proper')
  if (parsed.repack) flags.push('Repack')

  return (
    <div className="bg-card/50 rounded-lg p-4 space-y-2 border border-white/5">
      <div className="grid grid-cols-2 gap-x-6 gap-y-2">
        <DetailRow label="Resolution" value={parsed.resolution?.toUpperCase()} />
        <DetailRow label="Source" value={parsed.source && sourceLabels[parsed.source]} />
        <DetailRow label="Video Codec" value={parsed.videoCodec && codecLabels[parsed.videoCodec]} />
        <DetailRow label="Audio" value={parsed.audioCodec && audioLabels[parsed.audioCodec]} />
        <DetailRow label="HDR" value={parsed.hdr && hdrLabels[parsed.hdr]} />
        <DetailRow label="Streaming" value={parsed.streamingService} />
        <DetailRow
          label="Release Group"
          value={
            parsed.releaseGroup
              ? parsed.releaseGroupTier
                ? `${parsed.releaseGroup} (${tierLabels[parsed.releaseGroupTier]})`
                : parsed.releaseGroup
              : undefined
          }
        />
        <DetailRow label="Flags" value={flags.length > 0 ? flags.join(', ') : undefined} />
      </div>
    </div>
  )
}
