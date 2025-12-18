import { Hono } from 'hono'
import { MediaScanner } from '../services/scanner'
import { mediaService } from '../services/media'
import { TmdbClient } from '@jaeger/shared/clients/tmdb'
import { TvdbClient } from '@jaeger/shared/clients/tvdb'
import { MetadataRelayClient } from '@jaeger/shared/clients/metadata-relay'
import { MetadataProvider } from '@jaeger/shared/clients/metadata-provider'
import { RealFilesystem, MockFilesystem } from '../services/filesystem'

const media = new Hono()

media.get('/', async (c) => {
  const results = await mediaService.getAll()
  return c.json({ results })
})

media.post('/scan', async (c) => {
  const mockFilesystem = process.env.MOCK_FILESYSTEM === 'true'

  const providers = (process.env.METADATA_PROVIDERS || 'relay,tmdb,tvdb').split(',') as Array<
    'relay' | 'tmdb' | 'tvdb'
  >

  const clients: {
    tmdb?: TmdbClient
    tvdb?: TvdbClient
    relay?: MetadataRelayClient
  } = {}

  if (providers.includes('relay')) {
    clients.relay = new MetadataRelayClient(process.env.METADATA_RELAY_URL)
  }
  if (providers.includes('tmdb') && process.env.TMDB_API_KEY) {
    clients.tmdb = new TmdbClient(process.env.TMDB_API_KEY)
  }
  if (providers.includes('tvdb') && process.env.TVDB_API_KEY) {
    clients.tvdb = new TvdbClient(process.env.TVDB_API_KEY)
  }

  const metadataProvider = new MetadataProvider(
    {
      providers,
      tmdbApiKey: process.env.TMDB_API_KEY,
      tvdbApiKey: process.env.TVDB_API_KEY,
      relayUrl: process.env.METADATA_RELAY_URL,
    },
    clients
  )

  const filesystem = mockFilesystem ? new MockFilesystem() : new RealFilesystem()
  const scanner = new MediaScanner(metadataProvider, filesystem)

  const scanPath = mockFilesystem ? '/media' : process.env.MEDIA_PATH || '/media'

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
