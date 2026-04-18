import { eq, inArray } from 'drizzle-orm'
import { parseReleaseName } from '@jaeger/parser'
import { SABnzbdClient } from '@jaeger/shared/clients/sabnzbd'
import { db } from '../db'
import { downloads, requests } from '../db/schema'
import { configService } from './config'
import { logger } from '../lib/logger'
import { mkdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

export type DownloadStatus = 'fetching' | 'downloading' | 'complete' | 'failed' | 'cancelled' | 'replaced'

export type Download = {
  id: string
  requestId: string
  episodeId: string | null
  releaseName: string
  indexer: string | null
  size: number | null
  resolution: string | null
  source: string | null
  videoCodec: string | null
  audioCodec: string | null
  hdr: string | null
  releaseGroup: string | null
  qualityScore: number | null
  nzoId: string | null
  status: DownloadStatus
  failReason: string | null
  replacedById: string | null
  createdAt: Date
  completedAt: Date | null
}

export type DownloadWithRequest = Download & {
  requestTitle: string
  requestType: 'movie' | 'tv' | 'book'
}

function generateId(): string {
  return crypto.randomUUID()
}

function rowToDownload(row: typeof downloads.$inferSelect): Download {
  return {
    id: row.id,
    requestId: row.requestId,
    episodeId: row.episodeId,
    releaseName: row.releaseName,
    indexer: row.indexer,
    size: row.size,
    resolution: row.resolution,
    source: row.source,
    videoCodec: row.videoCodec,
    audioCodec: row.audioCodec,
    hdr: row.hdr,
    releaseGroup: row.releaseGroup,
    qualityScore: row.qualityScore,
    nzoId: row.nzoId,
    status: row.status as DownloadStatus,
    failReason: row.failReason,
    replacedById: row.replacedById,
    createdAt: row.createdAt,
    completedAt: row.completedAt,
  }
}

interface ReleaseInfo {
  releaseName: string
  downloadUrl: string
  indexer?: string
  size?: number
}

class DownloadService {
  private parseAndStore(releaseName: string) {
    const parsed = parseReleaseName(releaseName)
    return {
      resolution: parsed.resolution ?? null,
      source: parsed.isRemux ? 'remux' : (parsed.source ?? null),
      videoCodec: parsed.videoCodec ?? null,
      audioCodec: parsed.audioCodec ?? null,
      hdr: parsed.hdr ?? null,
      releaseGroup: parsed.releaseGroup ?? null,
      qualityScore: parsed.qualityScore ?? null,
    }
  }

  async createForRequest(requestId: string, release: ReleaseInfo): Promise<Download> {
    const id = generateId()
    const parsed = this.parseAndStore(release.releaseName)

    await db.insert(downloads).values({
      id,
      requestId,
      episodeId: null,
      releaseName: release.releaseName,
      indexer: release.indexer ?? null,
      size: release.size ?? null,
      ...parsed,
      nzoId: null,
      status: 'fetching',
      failReason: null,
      replacedById: null,
      createdAt: new Date(),
      completedAt: null,
    })

    const created = await this.getById(id)
    if (!created) throw new Error('Failed to create download')
    return created
  }

  async createForEpisode(requestId: string, episodeId: string, release: ReleaseInfo): Promise<Download> {
    const id = generateId()
    const parsed = this.parseAndStore(release.releaseName)

    await db.insert(downloads).values({
      id,
      requestId,
      episodeId,
      releaseName: release.releaseName,
      indexer: release.indexer ?? null,
      size: release.size ?? null,
      ...parsed,
      nzoId: null,
      status: 'fetching',
      failReason: null,
      replacedById: null,
      createdAt: new Date(),
      completedAt: null,
    })

    const created = await this.getById(id)
    if (!created) throw new Error('Failed to create download')
    return created
  }

  async createSeasonPack(requestId: string, release: ReleaseInfo): Promise<Download> {
    // Season packs are request-level downloads (episodeId = null)
    return this.createForRequest(requestId, release)
  }

  async sendToDownloader(downloadId: string, downloadUrl: string, category?: string): Promise<{ nzoId: string }> {
    const download = await this.getById(downloadId)
    if (!download) throw new Error('Download not found')

    const enabledDownloaders = configService.getEnabledDownloaders()
    if (enabledDownloaders.length === 0) {
      throw new Error('No downloaders configured')
    }

    const downloader = enabledDownloaders[0]!

    if (downloader.type === 'sabnzbd') {
      const client = new SABnzbdClient(downloader.url, downloader.apiKey)
      const result = await client.addNzbFromUrl(downloadUrl, download.releaseName, category)

      // Save NZB backup
      if (result.nzbContent) {
        const backupPath = configService.getNzbBackupPath()
        if (backupPath) {
          try {
            await mkdir(backupPath, { recursive: true })
            const safeFilename = download.releaseName.replace(/[<>:"/\\|?*]/g, '_')
            const filePath = join(backupPath, safeFilename.endsWith('.nzb') ? safeFilename : `${safeFilename}.nzb`)
            await writeFile(filePath, result.nzbContent, 'utf-8')
          } catch (error) {
            logger.error({ error }, 'Failed to save NZB backup')
          }
        }
      }

      if (result.nzoId) {
        await db.update(downloads).set({ nzoId: result.nzoId, status: 'downloading' }).where(eq(downloads.id, downloadId))

        // Also write to legacy request.nzoId for backward compat during transition
        await db.update(requests).set({ nzoId: result.nzoId, releaseName: download.releaseName }).where(eq(requests.id, download.requestId))

        return { nzoId: result.nzoId }
      }

      throw new Error('Downloader did not return nzoId')
    }

    throw new Error(`Unknown downloader type: ${downloader.type}`)
  }

  async getById(id: string): Promise<Download | null> {
    const rows = await db.select().from(downloads).where(eq(downloads.id, id)).limit(1)
    return rows[0] ? rowToDownload(rows[0]) : null
  }

  async getByRequestId(requestId: string): Promise<Download[]> {
    const rows = await db.select().from(downloads).where(eq(downloads.requestId, requestId))
    return rows.map(rowToDownload)
  }

  async getByEpisodeId(episodeId: string): Promise<Download[]> {
    const rows = await db.select().from(downloads).where(eq(downloads.episodeId, episodeId))
    return rows.map(rowToDownload)
  }

  async getActive(): Promise<DownloadWithRequest[]> {
    const rows = await db
      .select({
        download: downloads,
        requestTitle: requests.title,
        requestType: requests.type,
      })
      .from(downloads)
      .innerJoin(requests, eq(downloads.requestId, requests.id))
      .where(
        inArray(downloads.status, ['fetching', 'downloading'])
      )

    return rows.map((row) => ({
      ...rowToDownload(row.download),
      requestTitle: row.requestTitle,
      requestType: row.requestType as 'movie' | 'tv' | 'book',
    }))
  }

  async updateStatus(id: string, status: DownloadStatus, failReason?: string): Promise<void> {
    const updates: Record<string, unknown> = { status }
    if (status === 'complete') updates.completedAt = new Date()
    if (failReason) updates.failReason = failReason

    await db.update(downloads).set(updates).where(eq(downloads.id, id))
  }

  async cancel(id: string): Promise<void> {
    await db.update(downloads).set({ status: 'cancelled' }).where(eq(downloads.id, id))
  }

  async markReplaced(id: string, replacedById: string): Promise<void> {
    await db.update(downloads).set({ status: 'replaced', replacedById }).where(eq(downloads.id, id))
  }

  /**
   * Sync parent request status based on its downloads.
   * - Any download complete → request fulfilled
   * - All downloads failed/cancelled → request failed
   * - Active downloads → request stays as-is
   */
  async syncRequestStatus(requestId: string): Promise<void> {
    const allDownloads = await this.getByRequestId(requestId)
    if (allDownloads.length === 0) return

    const hasComplete = allDownloads.some((d) => d.status === 'complete')
    const hasActive = allDownloads.some((d) => d.status === 'fetching' || d.status === 'downloading')
    const allTerminal = allDownloads.every((d) =>
      d.status === 'complete' || d.status === 'failed' || d.status === 'cancelled' || d.status === 'replaced'
    )

    if (hasComplete) {
      await db.update(requests).set({ status: 'completed', completedAt: new Date() }).where(eq(requests.id, requestId))
    } else if (allTerminal && !hasActive) {
      await db.update(requests).set({ status: 'failed' }).where(eq(requests.id, requestId))
    }
  }

  /**
   * Get the best completed download for a request (highest quality score).
   */
  async getBestComplete(requestId: string): Promise<Download | null> {
    const allDownloads = await this.getByRequestId(requestId)
    const completed = allDownloads.filter((d) => d.status === 'complete')
    if (completed.length === 0) return null
    completed.sort((a, b) => (b.qualityScore ?? 0) - (a.qualityScore ?? 0))
    return completed[0]!
  }
}

export const downloadService = new DownloadService()
