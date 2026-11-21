import { describe, expect, test } from 'bun:test'
import {
  MetadataProvider,
  type MetadataProviderConfig,
} from '@fangst/shared/clients/metadata-provider'
import { MetadataRelayClient } from '@fangst/shared/clients/metadata-relay'
import { TmdbClient } from '@fangst/shared/clients/tmdb'
import { MockFilesystem } from '../services/filesystem'
import { MediaScanner } from '../services/scanner'

describe('MediaScanner E2E', () => {
  test('scans mock filesystem and matches metadata', async () => {
    const tmdbApiKey = process.env.TMDB_API_KEY
    const providers: MetadataProviderConfig['providers'] = ['relay']
    const clients: {
      relay?: MetadataRelayClient
      tmdb?: TmdbClient
    } = {}

    clients.relay = new MetadataRelayClient()

    if (tmdbApiKey) {
      clients.tmdb = new TmdbClient(tmdbApiKey)
      providers.push('tmdb')
    }

    const metadataProvider = new MetadataProvider({ providers }, clients)
    const filesystem = new MockFilesystem()
    const scanner = new MediaScanner(metadataProvider, filesystem)

    const results = await scanner.scanDirectory('/media')

    expect(results.length).toBeGreaterThan(0)

    const matrix = results.find((r) => r.parsed.title === 'The Matrix')
    expect(matrix).toBeDefined()
    expect(matrix?.parsed.year).toBe(1999)
    expect(matrix?.parsed.resolution).toBe('1080p')
    expect(matrix?.parsed.source).toBe('bluray')
    expect(matrix?.parsed.videoCodec).toBe('x264')

    const dune = results.find((r) => r.parsed.title === 'Dune')
    expect(dune).toBeDefined()
    expect(dune?.parsed.year).toBe(2021)
    expect(dune?.parsed.resolution).toBe('2160p')
    expect(dune?.parsed.hdr).toBe('dv')
    expect(dune?.parsed.audioCodec).toBe('atmos')

    const breakingBad = results.find((r) => r.parsed.title === 'Breaking Bad')
    expect(breakingBad).toBeDefined()
    expect(breakingBad?.parsed.season).toBe(1)
    expect(breakingBad?.parsed.episode).toBe(1)

    const bear = results.find((r) => r.parsed.title === 'The Bear')
    expect(bear).toBeDefined()
    expect(bear?.parsed.season).toBe(1)
    expect(bear?.parsed.episode).toBe(1)
    expect(bear?.parsed.resolution).toBe('2160p')

    if (matrix?.metadata) {
      expect(matrix.metadata.title).toBeTruthy()
      expect(matrix.metadata.provider).toBeTruthy()
    }

    if (dune?.metadata) {
      expect(dune.metadata.title).toBeTruthy()
      expect(dune.metadata.provider).toBeTruthy()
    }
  })
})
