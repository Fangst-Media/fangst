import { parseReleaseName } from '@fangst/parser'
import { isNotNull } from 'drizzle-orm'
import { db } from '../db'
import { downloads, episodes, requests } from '../db/schema'
import { logger } from '../lib/logger'

/**
 * Migrates existing request nzoId/releaseName data into the downloads table.
 * Idempotent — checks for existing downloads before creating.
 */
export async function migrateRequestsToDownloads(): Promise<void> {
  // Check if migration is needed (any requests with nzoId but no corresponding download)
  const requestsWithNzo = await db
    .select({
      id: requests.id,
      nzoId: requests.nzoId,
      releaseName: requests.releaseName,
      status: requests.status,
      createdAt: requests.createdAt,
      completedAt: requests.completedAt,
    })
    .from(requests)
    .where(isNotNull(requests.nzoId))

  if (requestsWithNzo.length === 0) return

  // Check existing downloads to avoid duplicates
  const existingDownloads = await db
    .select({ nzoId: downloads.nzoId })
    .from(downloads)
    .where(isNotNull(downloads.nzoId))

  const existingNzoIds = new Set(existingDownloads.map((d) => d.nzoId))

  let migrated = 0

  for (const request of requestsWithNzo) {
    if (!request.nzoId || existingNzoIds.has(request.nzoId)) continue

    const parsed = request.releaseName ? parseReleaseName(request.releaseName) : null

    const downloadStatus = mapRequestStatusToDownloadStatus(request.status)

    await db.insert(downloads).values({
      id: crypto.randomUUID(),
      requestId: request.id,
      episodeId: null,
      releaseName: request.releaseName ?? 'Unknown',
      indexer: null,
      size: null,
      resolution: parsed?.resolution ?? null,
      source: parsed?.isRemux ? 'remux' : (parsed?.source ?? null),
      videoCodec: parsed?.videoCodec ?? null,
      audioCodec: parsed?.audioCodec ?? null,
      hdr: parsed?.hdr ?? null,
      releaseGroup: parsed?.releaseGroup ?? null,
      qualityScore: parsed?.qualityScore ?? null,
      nzoId: request.nzoId,
      status: downloadStatus,
      failReason: null,
      replacedById: null,
      createdAt: request.createdAt,
      completedAt: request.completedAt,
    })

    migrated++
  }

  // Migrate episodes with nzoId
  const episodesWithNzo = await db
    .select({
      id: episodes.id,
      requestId: episodes.requestId,
      nzoId: episodes.nzoId,
      releaseName: episodes.releaseName,
      status: episodes.status,
      createdAt: episodes.createdAt,
    })
    .from(episodes)
    .where(isNotNull(episodes.nzoId))

  let episodeMigrated = 0

  for (const episode of episodesWithNzo) {
    if (!episode.nzoId || existingNzoIds.has(episode.nzoId)) continue

    const parsed = episode.releaseName ? parseReleaseName(episode.releaseName) : null

    const downloadStatus =
      episode.status === 'completed'
        ? ('complete' as const)
        : episode.status === 'failed'
          ? ('failed' as const)
          : episode.status === 'downloading'
            ? ('downloading' as const)
            : ('fetching' as const)

    await db.insert(downloads).values({
      id: crypto.randomUUID(),
      requestId: episode.requestId,
      episodeId: episode.id,
      releaseName: episode.releaseName ?? 'Unknown',
      indexer: null,
      size: null,
      resolution: parsed?.resolution ?? null,
      source: parsed?.isRemux ? 'remux' : (parsed?.source ?? null),
      videoCodec: parsed?.videoCodec ?? null,
      audioCodec: parsed?.audioCodec ?? null,
      hdr: parsed?.hdr ?? null,
      releaseGroup: parsed?.releaseGroup ?? null,
      qualityScore: parsed?.qualityScore ?? null,
      nzoId: episode.nzoId,
      status: downloadStatus,
      failReason: null,
      replacedById: null,
      createdAt: episode.createdAt,
      completedAt: downloadStatus === 'complete' ? episode.createdAt : null,
    })

    episodeMigrated++
  }

  if (migrated > 0 || episodeMigrated > 0) {
    logger.info(
      { migrated, episodeMigrated },
      'Migrated request/episode downloads to downloads table'
    )
  }
}

function mapRequestStatusToDownloadStatus(
  requestStatus: string
): 'fetching' | 'downloading' | 'complete' | 'failed' {
  switch (requestStatus) {
    case 'fetching_nzb':
    case 'nzb_downloaded':
    case 'sent_to_downloader':
      return 'fetching'
    case 'downloading':
      return 'downloading'
    case 'download_complete':
    case 'importing':
    case 'completed':
      return 'complete'
    case 'failed':
      return 'failed'
    default:
      return 'downloading'
  }
}
