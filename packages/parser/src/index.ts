export type ParsedRelease = {
  title: string
  year?: number
  season?: number
  episode?: number
  resolution?: '2160p' | '1080p' | '720p' | '480p'
  source?: 'bluray' | 'webdl' | 'webrip' | 'hdtv' | 'dvd'
  videoCodec?: 'x265' | 'x264' | 'av1' | 'xvid'
  audioCodec?: 'atmos' | 'truehd' | 'dts' | 'ddp' | 'aac' | 'ac3'
  hdr?: 'dv' | 'hdr10+' | 'hdr10'
  releaseGroup?: string
  proper?: boolean
  repack?: boolean
}

const patterns = {
  resolution: /\b(2160p|1080p|720p|480p)\b/i,
  source: /\b(blu-?ray|web-?dl|web-?rip|hdtv|dvd-?rip|bdrip|brrip)\b/i,
  videoCodec: /\b(x265|x\.?264|hevc|h\.?265|h\.?264|av1|xvid)\b/i,
  audioCodec: /\b(atmos|truehd|dts(?:-hd)?|ddp?(?:5\.1)?|aac|ac3)\b/i,
  hdr: /\b(dv|dolby[- ]?vision|hdr10\+?|hdr)\b/i,
  year: /\b(19\d{2}|20\d{2})\b/,
  seasonEpisode: /\bs(\d{1,2})e(\d{1,2})\b/i,
  releaseGroup: /-([a-z0-9]+)(?:\[.*?\])?$/i,
  proper: /\b(proper|real)\b/i,
  repack: /\brepack\b/i,
}

export function parseReleaseName(name: string): ParsedRelease {
  const result: ParsedRelease = {
    title: '',
  }

  const resolutionMatch = name.match(patterns.resolution)
  if (resolutionMatch?.[1]) {
    result.resolution = resolutionMatch[1].toLowerCase() as ParsedRelease['resolution']
  }

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

  if (/\batmos\b/i.test(name)) {
    result.audioCodec = 'atmos'
  } else {
    const audioMatch = name.match(patterns.audioCodec)
    if (audioMatch?.[1]) {
      const audio = audioMatch[1].toLowerCase().replace(/[.-]/g, '')
      const audioMap: Record<string, ParsedRelease['audioCodec']> = {
        truehd: 'truehd',
        dts: 'dts',
        dtshd: 'dts',
        ddp: 'ddp',
        ddp51: 'ddp',
        dd51: 'ddp',
        aac: 'aac',
        ac3: 'ac3',
      }
      result.audioCodec = audioMap[audio]
    }
  }

  const hdrMatch = name.match(patterns.hdr)
  if (hdrMatch?.[1]) {
    const hdr = hdrMatch[1].toLowerCase().replace(/[- ]/g, '')
    const hdrMap: Record<string, ParsedRelease['hdr']> = {
      dv: 'dv',
      dolbyvision: 'dv',
      'hdr10+': 'hdr10+',
      hdr10: 'hdr10',
      hdr: 'hdr10',
    }
    result.hdr = hdrMap[hdr]
  }

  const yearMatch = name.match(patterns.year)
  if (yearMatch?.[1]) {
    result.year = Number.parseInt(yearMatch[1])
  }

  const seMatch = name.match(patterns.seasonEpisode)
  if (seMatch?.[1] && seMatch?.[2]) {
    result.season = Number.parseInt(seMatch[1])
    result.episode = Number.parseInt(seMatch[2])
  }

  const groupMatch = name.match(patterns.releaseGroup)
  if (groupMatch) {
    result.releaseGroup = groupMatch[1]
  }

  result.proper = patterns.proper.test(name)
  result.repack = patterns.repack.test(name)

  let titleEnd = name.length
  if (yearMatch?.index) titleEnd = Math.min(titleEnd, yearMatch.index)
  if (seMatch?.index) titleEnd = Math.min(titleEnd, seMatch.index)

  result.title = name.slice(0, titleEnd).replace(/[._]/g, ' ').trim().replace(/\s+/g, ' ')

  return result
}
