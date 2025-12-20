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

media.get('/', async (c) => {
  const results = await mediaService.getAll()
  return c.json({ results })
})

media.post('/scan', async (c) => {
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

  try {
    const scannedFiles = await scanner.scanDirectory(scanPath)
    const syncResult = await mediaService.syncFromScan(scannedFiles)
    const results = await mediaService.getAll()

    return c.json({
      mockFilesystem,
      providers,
      scannedPath: scanPath,
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
