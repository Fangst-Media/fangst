import { describe, expect, test } from 'bun:test'
import {
  detectStreamingService,
  getReleaseGroupTier,
  parseReleaseName,
} from './index'

describe('parseReleaseName', () => {
  test('parses movie release', () => {
    const result = parseReleaseName('The.Matrix.1999.1080p.BluRay.x264-GROUP')

    expect(result.title).toBe('The Matrix')
    expect(result.year).toBe(1999)
    expect(result.resolution).toBe('1080p')
    expect(result.source).toBe('bluray')
    expect(result.videoCodec).toBe('x264')
    expect(result.releaseGroup).toBe('GROUP')
  })

  test('parses TV show release', () => {
    const result = parseReleaseName('Breaking.Bad.S01E01.1080p.WEB-DL.DD5.1.x264-GROUP')

    expect(result.title).toBe('Breaking Bad')
    expect(result.season).toBe(1)
    expect(result.episode).toBe(1)
    expect(result.resolution).toBe('1080p')
    expect(result.source).toBe('webdl')
    expect(result.videoCodec).toBe('x264')
  })

  test('parses 4K HDR release', () => {
    const result = parseReleaseName('Dune.2021.2160p.WEB-DL.DDP5.1.Atmos.DV.HDR10.HEVC-GROUP')

    expect(result.title).toBe('Dune')
    expect(result.year).toBe(2021)
    expect(result.resolution).toBe('2160p')
    expect(result.source).toBe('webdl')
    expect(result.videoCodec).toBe('x265')
    expect(result.audioCodec).toBe('atmos')
    expect(result.hdr).toBe('dv')
  })

  test('parses proper/repack flags', () => {
    const proper = parseReleaseName('Movie.2023.1080p.PROPER.WEB-DL.x264-GROUP')
    const repack = parseReleaseName('Movie.2023.1080p.REPACK.WEB-DL.x264-GROUP')

    expect(proper.proper).toBe(true)
    expect(repack.repack).toBe(true)
  })

  test('detects remux releases', () => {
    const result = parseReleaseName('Movie.2023.2160p.UHD.BluRay.Remux.TrueHD.7.1.Atmos-FraMeSToR')

    expect(result.isRemux).toBe(true)
    expect(result.resolution).toBe('2160p')
    expect(result.audioCodec).toBe('atmos')
  })
})

describe('streaming service detection', () => {
  test('detects Netflix', () => {
    const result = parseReleaseName('Movie.2024.1080p.NF.WEB-DL.DDP5.1-GROUP')
    expect(result.streamingService).toBe('NF')
  })

  test('detects Amazon', () => {
    const result = parseReleaseName('Movie.2024.2160p.AMZN.WEB-DL.DDP5.1.DV.HDR-GROUP')
    expect(result.streamingService).toBe('AMAZ')
  })

  test('detects Disney+', () => {
    const result = parseReleaseName('Show.S01E01.1080p.DSNP.WEB-DL.DDP5.1-GROUP')
    expect(result.streamingService).toBe('DSNP')
  })

  test('detects Apple TV+', () => {
    const result = parseReleaseName('Show.S01E01.2160p.ATVP.WEB-DL.DDP5.1.Atmos.DV-GROUP')
    expect(result.streamingService).toBe('ATVP')
  })

  test('returns undefined for unknown service', () => {
    const result = parseReleaseName('Movie.2024.1080p.BluRay.x264-GROUP')
    expect(result.streamingService).toBeUndefined()
  })
})

describe('release group tiers', () => {
  test('identifies tier 1 release groups', () => {
    const hallowed = parseReleaseName('Movie.2024.1080p.BluRay.x264-hallowed')
    expect(hallowed.releaseGroupTier).toBe(1)

    const flux = parseReleaseName('Movie.2024.1080p.NF.WEB-DL.DDP5.1-FLUX')
    expect(flux.releaseGroupTier).toBe(1)
  })

  test('identifies tier 2 release groups', () => {
    const bhdstudio = parseReleaseName('Movie.2024.1080p.BluRay.x264-BHDStudio')
    expect(bhdstudio.releaseGroupTier).toBe(2)
  })

  test('returns null for unknown groups', () => {
    const unknown = parseReleaseName('Movie.2024.1080p.BluRay.x264-UNKNOWNGROUP')
    expect(unknown.releaseGroupTier).toBeNull()
  })
})

describe('quality scoring', () => {
  test('scores remux 4K release highest', () => {
    const remux = parseReleaseName('Movie.2024.2160p.UHD.BluRay.Remux.TrueHD.Atmos-FraMeSToR')
    // 2160p (40) + remux (25) + truehd (9) = 74 base, but atmos overrides to 10
    // Actually: 2160p (40) + remux (25) + atmos (10) + tier varies
    expect(remux.qualityScore).toBeGreaterThanOrEqual(75)
  })

  test('scores 1080p bluray tier 1 higher than unknown group', () => {
    const tier1 = parseReleaseName('Movie.2024.1080p.BluRay.x264-hallowed')
    const unknown = parseReleaseName('Movie.2024.1080p.BluRay.x264-UNKNOWNGROUP')

    expect(tier1.qualityScore).toBeGreaterThan(unknown.qualityScore ?? 0)
  })

  test('scores 4K higher than 1080p', () => {
    const uhd = parseReleaseName('Movie.2024.2160p.WEB-DL.DDP5.1-GROUP')
    const hd = parseReleaseName('Movie.2024.1080p.WEB-DL.DDP5.1-GROUP')

    expect(uhd.qualityScore).toBeGreaterThan(hd.qualityScore ?? 0)
  })

  test('proper releases get bonus points', () => {
    const regular = parseReleaseName('Movie.2024.1080p.WEB-DL.x264-GROUP')
    const proper = parseReleaseName('Movie.2024.1080p.PROPER.WEB-DL.x264-GROUP')

    expect(proper.qualityScore).toBeGreaterThan(regular.qualityScore ?? 0)
  })
})

describe('detectStreamingService', () => {
  test('detects common streaming services', () => {
    expect(detectStreamingService('Movie.2024.NF.WEB-DL')).toBe('NF')
    expect(detectStreamingService('Movie.2024.Netflix.WEB-DL')).toBe('NF')
    expect(detectStreamingService('Movie.2024.Hulu.WEB-DL')).toBe('HULU')
  })
})

describe('getReleaseGroupTier', () => {
  test('returns tier for known groups', () => {
    expect(getReleaseGroupTier('hallowed', '1080p', 'bluray').tier).toBe(1)
    expect(getReleaseGroupTier('FLUX', '1080p', 'web_dl').tier).toBe(1)
  })

  test('returns null for unknown groups', () => {
    expect(getReleaseGroupTier('UNKNOWNGROUP', '1080p', 'bluray').tier).toBeNull()
  })

  test('is case insensitive', () => {
    expect(getReleaseGroupTier('HALLOWED', '1080p', 'bluray').tier).toBe(1)
    expect(getReleaseGroupTier('Hallowed', '1080p', 'bluray').tier).toBe(1)
  })
})
