import { basename, extname } from 'node:path'
import { parseReleaseName } from '@jaeger/parser'
import type { MetadataProvider, MetadataResult } from '@jaeger/shared/clients/metadata-provider'
import type { IFilesystem } from './filesystem'

export type ScannedFile = {
  path: string
  filename: string
  size?: number
  parsed: ReturnType<typeof parseReleaseName>
  metadata?: MetadataResult
}

export class MediaScanner {
  constructor(
    private metadataProvider: MetadataProvider,
    private filesystem: IFilesystem
  ) {}

  async scanDirectory(rootPath: string): Promise<ScannedFile[]> {
    const results: ScannedFile[] = []
    await this.walkDirectory(rootPath, results)
    return results
  }

  private async walkDirectory(dirPath: string, results: ScannedFile[]): Promise<void> {
    const entries = await this.filesystem.readDirectory(dirPath)

    for (const entry of entries) {
      if (entry.isDirectory) {
        await this.walkDirectory(entry.path, results)
      } else if (this.filesystem.isVideoFile(entry.name)) {
        const filename = basename(entry.name, extname(entry.name))
        const parsed = parseReleaseName(filename)

        const scanned: ScannedFile = {
          path: entry.path,
          filename: entry.name,
          size: entry.size,
          parsed,
        }

        try {
          const metadata = await this.metadataProvider.search(
            parsed.title,
            parsed.year,
            parsed.season !== undefined ? 'tv' : 'movie'
          )
          if (metadata) {
            scanned.metadata = metadata
          }
        } catch (error) {
          console.error(`Failed to lookup metadata for ${filename}:`, error)
        }

        results.push(scanned)
      }
    }
  }
}
