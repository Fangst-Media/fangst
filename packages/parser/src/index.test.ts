import { describe, expect, test } from 'bun:test'
import { parseReleaseName } from './index'

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
})
