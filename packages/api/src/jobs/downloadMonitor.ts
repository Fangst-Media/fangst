import { requestService } from '../services/requests'
import { downloadService } from '../services/downloads'
import { configService } from '../services/config'
import { SABnzbdClient, type SABnzbdHistorySlot } from '@jaeger/shared/clients/sabnzbd'
import { runLibraryScan } from '../routes/media'
import { logger } from '../lib/logger'

export async function runDownloadMonitor(): Promise<{
  checked: number
  completed: number
  failed: number
  librarySync?: { added: number; updated: number; removed: number }
}> {
  logger.info('Starting download monitor job')

  // Check new-style downloads first
  const activeDownloads = await downloadService.getActive()

  // Also check legacy requests that have nzoId but no download row
  const legacyDownloading = await requestService.getDownloading()

  const totalChecks = activeDownloads.length + legacyDownloading.length

  if (totalChecks === 0) {
    return { checked: 0, completed: 0, failed: 0 }
  }

  const downloaders = configService.getEnabledDownloaders()
  if (downloaders.length === 0) {
    logger.warn('No downloaders configured')
    return { checked: 0, completed: 0, failed: 0 }
  }

  let completed = 0
  let failed = 0

  // Collect history from all SABnzbd downloaders
  const allHistory: SABnzbdHistorySlot[] = []

  for (const downloader of downloaders) {
    if (downloader.type === 'sabnzbd') {
      try {
        const client = new SABnzbdClient(downloader.url, downloader.apiKey)
        const history = await client.getHistory(50)
        allHistory.push(...history.slots)
      } catch (error) {
        logger.error({ error, downloader: downloader.name }, 'Failed to get history from downloader')
      }
    }
  }

  logger.info({ historyItems: allHistory.length }, 'Got SABnzbd history')

  // Check new-style downloads
  for (const download of activeDownloads) {
    if (!download.nzoId) continue

    const match = allHistory.find((slot) => slot.id === download.nzoId)
    if (!match) continue

    if (match.status === 'Completed') {
      await downloadService.updateStatus(download.id, 'complete')
      await downloadService.syncRequestStatus(download.requestId)
      logger.info({ title: download.requestTitle, release: download.releaseName }, 'Download completed')
      completed++
    } else if (match.status === 'Failed') {
      await downloadService.updateStatus(download.id, 'failed', match.failMessage ?? 'Download failed')
      await downloadService.syncRequestStatus(download.requestId)
      logger.info({ title: download.requestTitle, reason: match.failMessage }, 'Download failed')
      failed++
    }
  }

  // Check legacy requests (backward compat during transition)
  for (const request of legacyDownloading) {
    if (!request.nzoId) continue

    // Skip if already tracked by a download row
    const existingDownloads = await downloadService.getByRequestId(request.id)
    const trackedByDownload = existingDownloads.some((d) => d.nzoId === request.nzoId && (d.status === 'downloading' || d.status === 'fetching'))
    if (trackedByDownload) continue

    const match = allHistory.find((slot) => slot.id === request.nzoId)
    if (!match) continue

    if (match.status === 'Completed') {
      await requestService.updateStatus(request.id, 'completed')
      logger.info({ title: request.title }, 'Legacy download completed')
      completed++
    } else if (match.status === 'Failed') {
      await requestService.updateStatus(request.id, 'failed')
      logger.info({ title: request.title, reason: match.failMessage }, 'Legacy download failed')
      failed++
    }
  }

  logger.info({ checked: totalChecks, completed, failed }, 'Download monitor complete')

  // Trigger library scan if any downloads completed
  let librarySync: { added: number; updated: number; removed: number } | undefined
  if (completed > 0) {
    try {
      librarySync = await runLibraryScan()
      logger.info({ ...librarySync }, 'Library scan after download completion')
    } catch (error) {
      logger.error({ error }, 'Library scan failed')
    }
  }

  return { checked: totalChecks, completed, failed, librarySync }
}
