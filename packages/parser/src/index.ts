import { detectStreamingService } from './data/streaming-services'
import { getReleaseGroupTier } from './data/release-groups'
import { calculateQualityScore } from './scoring'

export type ParsedRelease = {
  title: string
  year?: number
  season?: number
  episode?: number
  resolution?: '2160p' | '1080p' | '720p' | '480p'
  source?: 'bluray' | 'webdl' | 'webrip' | 'hdtv' | 'dvd'
  videoCodec?: 'x265' | 'x264' | 'av1' | 'xvid'
  audioCodec?: 'atmos' | 'truehd' | 'dtsx' | 'dtshd' | 'dts' | 'ddp' | 'dd' | 'flac' | 'aac' | 'ac3' | 'opus'
  hdr?: 'dv' | 'hdr10+' | 'hdr10' | 'hlg'
  releaseGroup?: string
  proper?: boolean
  repack?: boolean
  // New fields from Dictionarry integration
  streamingService?: string
  releaseGroupTier?: 1 | 2 | 3 | null
  isRemux?: boolean
  qualityScore?: number
}

const patterns = {
  resolution: /\b(2160p|1080p|720p|480p)\b/i,
  source: /\b(blu-?ray|web-?dl|web-?rip|hdtv|dvd-?rip|bdrip|brrip)\b/i,
  remux: /\bremux\b/i,
  videoCodec: /\b(x265|x\.?264|hevc|h\.?265|h\.?264|av1|xvid)\b/i,
  audioCodec: /\b(truehd|dts-?x|dts-?hd(?:\s*ma)?|dts|ddp|dd|flac|aac|ac3|opus)\b/i,
  hdr: /\b(dv|dovi|dolby[- ]?vision|hdr10\+|hdr10|hdr|hlg)\b/i,
  year: /\b(19\d{2}|20\d{2})\b/,
  seasonEpisode: /\bs(\d{1,2})e(\d{1,2})\b/i,
  seasonOnly: /\bs(\d{1,2})\b(?!e)/i,
  releaseGroup: /-([a-z0-9]+)(?:\[.*?\])?$/i,
  proper: /\b(proper|real)\b/i,
  repack: /\brepack\b/i,
}

export function parseReleaseName(name: string): ParsedRelease {
  const result: ParsedRelease = {
    title: '',
  }

  // Resolution
  const resolutionMatch = name.match(patterns.resolution)
  if (resolutionMatch?.[1]) {
    result.resolution = resolutionMatch[1].toLowerCase() as ParsedRelease['resolution']
  }

  // Source
  const sourceMatch = name.match(patterns.source)
  if (sourceMatch?.[1]) {
    const source = sourceMatch[1].toLowerCase().replace(/[.-]/g, '')
    const sourceMap: Record<string, ParsedRelease['source']> = {
      bluray: 'bluray',
      bdrip: 'bluray',
      brrip: 'bluray',
      webdl: 'webdl',
      webrip: 'webrip',
      hdtv: 'hdtv',
      dvdrip: 'dvd',
    }
    result.source = sourceMap[source]
  }

  // Remux detection
  result.isRemux = patterns.remux.test(name)

  // Streaming service (from Dictionarry patterns)
  result.streamingService = detectStreamingService(name)

  // Video codec
  const videoMatch = name.match(patterns.videoCodec)
  if (videoMatch?.[1]) {
    const codec = videoMatch[1].toLowerCase().replace(/[.]/g, '')
    const codecMap: Record<string, ParsedRelease['videoCodec']> = {
      x265: 'x265',
      hevc: 'x265',
      h265: 'x265',
      x264: 'x264',
      h264: 'x264',
      av1: 'av1',
      xvid: 'xvid',
    }
    result.videoCodec = codecMap[codec]
  }

  // Audio codec - Atmos is checked separately as it appears alongside other audio codecs
  if (/\batmos\b/i.test(name)) {
    result.audioCodec = 'atmos'
  } else {
    const audioMatch = name.match(patterns.audioCodec)
    if (audioMatch?.[1]) {
      const audio = audioMatch[1].toLowerCase().replace(/[.-\s]/g, '')
      const audioMap: Record<string, ParsedRelease['audioCodec']> = {
        truehd: 'truehd',
        dtsx: 'dtsx',
        dtshd: 'dtshd',
        dtshdma: 'dtshd',
        dts: 'dts',
        ddp: 'ddp',
        dd: 'dd',
        flac: 'flac',
        aac: 'aac',
        ac3: 'ac3',
        opus: 'opus',
      }
      result.audioCodec = audioMap[audio]
    }
  }

  // HDR format
  const hdrMatch = name.match(patterns.hdr)
  if (hdrMatch?.[1]) {
    const hdr = hdrMatch[1].toLowerCase().replace(/[- ]/g, '')
    const hdrMap: Record<string, ParsedRelease['hdr']> = {
      dv: 'dv',
      dovi: 'dv',
      dolbyvision: 'dv',
      'hdr10+': 'hdr10+',
      hdr10: 'hdr10',
      hdr: 'hdr10',
      hlg: 'hlg',
    }
    result.hdr = hdrMap[hdr]
  }

  // Year
  const yearMatch = name.match(patterns.year)
  if (yearMatch?.[1]) {
    result.year = Number.parseInt(yearMatch[1])
  }

  // Season/Episode
  const seMatch = name.match(patterns.seasonEpisode)
  if (seMatch?.[1] && seMatch?.[2]) {
    result.season = Number.parseInt(seMatch[1])
    result.episode = Number.parseInt(seMatch[2])
  } else {
    // Try season only (e.g., "S01" without episode)
    const seasonMatch = name.match(patterns.seasonOnly)
    if (seasonMatch?.[1]) {
      result.season = Number.parseInt(seasonMatch[1])
    }
  }

  // Release group and tier (from Dictionarry data)
  const groupMatch = name.match(patterns.releaseGroup)
  if (groupMatch) {
    result.releaseGroup = groupMatch[1]

    // Map source for tier lookup
    let tierSource = 'any'
    if (result.source === 'webdl' || result.source === 'webrip') {
      tierSource = 'web_dl'
    } else if (result.source === 'bluray') {
      tierSource = 'bluray'
    } else if (result.source === 'dvd') {
      tierSource = 'dvd'
    }

    // @ts-expect-error - TS 5.9 has issues with literal union to string | undefined assignment
    const tierInfo = getReleaseGroupTier(groupMatch[1], result.resolution, tierSource)
    result.releaseGroupTier = tierInfo.tier
  }

  // Proper/Repack flags
  result.proper = patterns.proper.test(name)
  result.repack = patterns.repack.test(name)

  // Extract title (everything before year or season/episode marker)
  let titleEnd = name.length
  if (yearMatch?.index) titleEnd = Math.min(titleEnd, yearMatch.index)
  if (seMatch?.index) titleEnd = Math.min(titleEnd, seMatch.index)

  result.title = name.slice(0, titleEnd).replace(/[._]/g, ' ').trim().replace(/\s+/g, ' ')

  // Calculate quality score
  result.qualityScore = calculateQualityScore(result)

  return result
}

// Re-export utilities for direct use
export { detectStreamingService } from './data/streaming-services'
export { getReleaseGroupTier, type ReleaseGroupInfo } from './data/release-groups'
export { calculateQualityScore } from './scoring'
