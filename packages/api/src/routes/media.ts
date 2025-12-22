import { Hono } from 'hono'
import { MediaScanner } from '../services/scanner'
import { mediaService } from '../services/media'
import { TmdbClient } from '@jaeger/shared/clients/tmdb'
import { TvdbClient } from '@jaeger/shared/clients/tvdb'
import { MetadataRelayClient } from '@jaeger/shared/clients/metadata-relay'
import { MetadataProvider } from '@jaeger/shared/clients/metadata-provider'
import { RealFilesystem, MockFilesystem } from '../services/filesystem'
import { configService } from '../services/config'

const media = new Hono()

export async function runLibraryScan(): Promise<{
  added: number
  updated: number
  removed: number
}> {
  const mockFilesystem = configService.isMockFilesystem()
  const providers = configService.getMetadataProviders()

  const clients: {
    tmdb?: TmdbClient
    tvdb?: TvdbClient
    relay?: MetadataRelayClient
  } = {}

  const relayUrl = configService.getMetadataRelayUrl()
  const tmdbApiKey = configService.getTmdbApiKey()
  const tvdbApiKey = configService.getTvdbApiKey()

  if (providers.includes('relay')) {
    clients.relay = new MetadataRelayClient(relayUrl)
  }
  if (providers.includes('tmdb') && tmdbApiKey) {
    clients.tmdb = new TmdbClient(tmdbApiKey)
  }
  if (providers.includes('tvdb') && tvdbApiKey) {
    clients.tvdb = new TvdbClient(tvdbApiKey)
  }

  const metadataProvider = new MetadataProvider(
    {
      providers,
      tmdbApiKey,
      tvdbApiKey,
      relayUrl,
    },
    clients
  )

  const filesystem = mockFilesystem ? new MockFilesystem() : new RealFilesystem()
  const scanner = new MediaScanner(metadataProvider, filesystem)

  const scanPath = mockFilesystem ? '/media' : configService.getMediaPath()

  const scannedFiles = await scanner.scanDirectory(scanPath)
  return mediaService.syncFromScan(scannedFiles)
}

media.get('/', async (c) => {
  const results = await mediaService.getAll()
  return c.json({ results })
})

media.post('/scan', async (c) => {
  try {
    const syncResult = await runLibraryScan()
    const results = await mediaService.getAll()

    return c.json({
      mockFilesystem: configService.isMockFilesystem(),
      providers: configService.getMetadataProviders(),
      scannedPath: configService.isMockFilesystem() ? '/media' : configService.getMediaPath(),
      sync: syncResult,
      results,
    })
  } catch (error) {
    return c.json(
      {
        error: 'Failed to scan media',
        message: error instanceof Error ? error.message : 'Unknown error',
      },
      500
    )
  }
})

export default media
